//! Which store belongs to which of the game's bags.
//!
//! # The view the game is given
//!
//! The game reaches a bag through an accessor, every time it needs one,
//! including once per frame while drawing the inventory panel. That is the
//! point where a larger inventory becomes possible: the accessor is replaced,
//! and it answers with a sixty-four byte bag this mod owns rather than the one
//! inside the game's object.
//!
//! Writing into the game's own bag instead was tried first and does not work
//! for anything the player can see. The panel is drawn from whatever the
//! accessor returned, so changing memory the accessor did not point at changes
//! nothing on screen.
//!
//! # Keeping the two in step
//!
//! The game writes into the view it was handed, at sites this mod does not
//! intercept. So every accessor call reads the view back into the store before
//! refreshing it from the store. Whatever the game wrote at visible slot `k` is
//! still at store index `position + k`, because the window only moves while we
//! hold it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::inventory::{Bag, Item, BAG_SIZE};
use crate::store::window::Window;

/// One character's storage, and the bag the game is shown in its place.
struct Entry {
    /// The object holding the game's own bag, and the offset within it. Together
    /// these identify a character's inventory across frames, which a heap
    /// address on its own does not.
    owner: usize,
    offset: usize,

    /// The bag handed to the game.
    ///
    /// A raw allocation rather than a `Box`, and never turned into a reference.
    /// The game writes into this while it is ours, and a Rust reference to
    /// memory something else is writing is undefined behaviour: the compiler is
    /// entitled to assume nothing else touches it and to keep values in
    /// registers across the writes. Every access goes through a volatile read
    /// or write of the whole structure.
    view: *mut Bag,

    /// What the game's own bag held last time this entry was looked at.
    ///
    /// Not every write goes through the accessor. Loading a save, in
    /// particular, may drop an inventory straight into the game's object. That
    /// would otherwise be invisible here, and the store would overwrite it with
    /// stale contents the next time the panel was drawn.
    mirror: Bag,

    window: Window,

    /// Last state reported to the log, so only changes are printed.
    last_reported: String,
}

impl Entry {
    /// Takes a copy of the bag the game has been writing into.
    fn read_view(&self) -> Bag {
        // Safety: the allocation is this entry's and outlives it.
        unsafe { read(self.view) }
    }

    /// Publishes a bag for the game to read.
    fn write_view(&self, bag: &Bag) {
        unsafe { write(self.view, bag) };
    }

    /// Refreshes the view from the store, after taking in whatever the game
    /// wrote into it since last time.
    fn sync(&mut self) {
        self.sync_with(|_| {});
    }

    /// `sync`, with a change to the window between taking in and publishing.
    fn sync_with(&mut self, adjust: impl FnOnce(&mut Window)) {
        let mut bag = self.read_view();
        self.window.read_from(&bag);
        adjust(&mut self.window);
        self.window.write_into(&mut bag);
        self.write_view(&bag);
    }

    /// Out in the world the game reads the equipped index off this bag and
    /// holsters what it cannot see. The screen is the only place the window
    /// may rest away from the equipped item; everywhere else it comes back.
    fn sync_for_the_world(&mut self) {
        let mut moved = false;
        self.sync_with(|window| moved = window.keep_equipped_visible());

        if moved {
            log_info!(
                "Bag +0x{:02X}: window back on the equipped item, slot {}.",
                self.offset,
                self.window.position() + 1
            );
        }
    }

    /// Starts the store over from a bag the game filled in behind our back.
    fn reseed(&mut self, source: &Bag) {
        self.window = Window::new(self.window.store().capacity());
        self.window.read_from(source);
        self.write_view(source);
    }

    /// Reports what the game is about to be shown, when it changes.
    ///
    /// The accessor runs once per drawn frame, so this only speaks up when the
    /// answer is different from last time. Without that it would be thousands of
    /// identical lines; with it, every line is a change worth explaining.
    fn report(&mut self) {
        let bag = self.read_view();
        let ids: Vec<i32> = bag.items.iter().map(|item| item.id).collect();
        let fingerprint = format!("{}:{:?}", self.window.position(), ids);

        if self.last_reported == fingerprint {
            return;
        }
        self.last_reported = fingerprint;

        log_info!(
            "View 0x{:08X}+0x{:02X} at slot {}: {:?}  equipped {}",
            self.owner,
            self.offset,
            self.window.position() + 1,
            ids,
            bag.equipped_index
        );
    }
}

// Safety: the pointer is a plain allocation reached only through the volatile
// helpers below, and it is never freed. The reason it is a raw pointer is
// aliasing with the game, not thread ownership.
unsafe impl Send for Entry {}

// The view allocation is deliberately never freed.
//
// The game is handed its address and keeps it for as long as it likes, with no
// way for this mod to know when the last copy is gone. Freeing on drop would
// turn every stale copy into a use-after-free, and there are at most a handful
// of these for the life of the process. Leaking is the cheaper mistake.

/// Copies a bag out of memory something else may be writing.
///
/// # Safety
/// `at` must point at a readable, correctly aligned `Bag`.
unsafe fn read(at: *const Bag) -> Bag {
    at.read_volatile()
}

/// Copies a bag into memory something else may be reading.
///
/// # Safety
/// `at` must point at a writable, correctly aligned `Bag`.
unsafe fn write(at: *mut Bag, value: &Bag) {
    at.write_volatile(*value);
}

pub struct Registry {
    entries: Vec<Entry>,
    /// Slots each store gets. Read from the configuration at startup.
    capacity: usize,
}

impl Registry {
    const fn new() -> Registry {
        Registry {
            entries: Vec::new(),
            capacity: 0,
        }
    }

    fn find(&mut self, owner: usize, offset: usize) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.owner == owner && e.offset == offset)
    }
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry::new());

/// Sets how many slots each store gets. Call once, before any hook runs.
pub fn set_capacity(slots: usize) {
    if let Ok(mut registry) = REGISTRY.lock() {
        registry.capacity = slots;
        log_debug!("Store capacity set to {slots} slots.");
    }
}

/// The bag to hand the game in place of the one at `owner + offset`.
///
/// The first time a character's inventory is seen, its store is seeded from
/// what the game already has there, so an inventory that exists before the mod
/// looks at it is inherited rather than emptied.
///
/// Returns null if the store is unavailable, which the caller turns back into
/// the game's own bag. Handing over a half-initialised view would be worse than
/// not modding that call at all.
///
/// # Safety
/// `owner + offset` must point at a readable `Bag`.
pub unsafe fn view_for(owner: usize, offset: usize) -> *mut Bag {
    let Ok(mut registry) = REGISTRY.lock() else {
        return std::ptr::null_mut();
    };

    // The game's own bag, only ever touched through raw reads and writes. The
    // game writes it too, so a reference here would be a promise this code
    // cannot keep.
    let own_ptr = (owner + offset) as *mut Bag;
    let own = read(own_ptr);

    let index = match registry.find(owner, offset) {
        Some(index) => index,
        None => {
            let capacity = registry.capacity;

            let mut window = Window::new(capacity);
            window.read_from(&own);

            log_info!(
                "New store for the bag at 0x{:08X}+0x{:02X}: {} slots.",
                owner,
                offset,
                window.store().capacity()
            );

            registry.entries.push(Entry {
                owner,
                offset,
                view: Box::into_raw(Box::new(own)),
                mirror: own,
                window,
                last_reported: String::new(),
            });

            registry.entries.len() - 1
        }
    };

    let entry = &mut registry.entries[index];

    // Someone wrote the game's own bag without going through here. Whatever it
    // put there is newer than anything the store holds, so the store restarts
    // from it rather than overwriting a freshly loaded inventory.
    if entry.mirror != own {
        log_info!(
            "Bag at 0x{:08X}+0x{:02X} changed underneath us; reseeding.",
            owner,
            offset
        );
        entry.reseed(&own);
    }

    // A staged restore may be waiting whatever path led here: a reseed when a
    // load overwrote the bag mid-session, a brand-new entry when the save was
    // loaded straight from the main menu, or no visible change at all when the
    // loaded six happen to equal what was already showing. It verifies itself
    // against the game's bag, so offering it every time is safe — and offering
    // it only after a reseed missed the main-menu path entirely.
    apply_staged(entry, &own);

    if crate::hook::panel::screen_holds_the_window() {
        entry.sync();
    } else {
        entry.sync_for_the_world();
    }

    // Keep the game's own bag showing what the view shows. Code that reaches it
    // without an accessor then sees the visible slots rather than a stale copy,
    // and this is also what makes the comparison above mean anything.
    let published = entry.read_view();
    write(own_ptr, &published);
    entry.mirror = published;

    entry.report();

    entry.view
}

/// Forgets every store.
///
/// The object a store is keyed on is freed and rebuilt on a new game or a load.
/// Keeping one keyed on a stale address would hand a character's items to
/// whatever is allocated there next.
///
/// The views themselves are not freed, and cannot be: the game holds their
/// addresses. Dropping the entries only drops this mod's claim on them.
pub fn forget_all() {
    if let Ok(mut registry) = REGISTRY.lock() {
        let count = registry.entries.len();
        registry.entries.clear();
        log_info!("Dropped {count} store(s).");
    }
}

/// The state of this module's locks, for the hang report.
pub fn lock_states() -> [(&'static str, &'static str); 2] {
    use crate::debug::hang::describe_lock;
    [
        ("registry", describe_lock(&REGISTRY)),
        ("staged restores", describe_lock(&PENDING)),
    ]
}

/// Scrolls every store by `rows` rows of two.
///
/// Every store moves together rather than only the panel being looked at. The
/// panels sit side by side, so moving both shows the same rows of each, and it
/// sidesteps a question the game has not answered yet: which character a given
/// panel belongs to. Getting that wrong would scroll the other character's
/// inventory, which is worse than scrolling both.
///
/// The view is refreshed here rather than waiting for the next accessor call,
/// so a scroll is visible on the very next frame.
///
/// Returns how many stores actually moved.
pub fn scroll_all(rows: i32) -> usize {
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };

    let mut moved = 0;

    for entry in registry.entries.iter_mut() {
        // Take in whatever the game wrote before moving, or the slots currently
        // on screen would be lost.
        let mut bag = entry.read_view();
        entry.window.read_from(&bag);

        if !entry.window.scroll_rows(rows) {
            continue;
        }

        entry.window.write_into(&mut bag);
        entry.write_view(&bag);
        moved += 1;
    }

    moved
}

// --- Holding a row still for the inventory's two-item actions ---

/// Runs `action` on every store at bag offset `offset`, through the usual
/// read-back and publish, and reports whether any store was found.
fn with_offset(offset: usize, mut action: impl FnMut(&mut Window)) -> bool {
    let Ok(mut registry) = REGISTRY.lock() else {
        return false;
    };

    let mut found = false;

    for entry in registry.entries.iter_mut().filter(|e| e.offset == offset) {
        let mut bag = entry.read_view();
        entry.window.read_from(&bag);
        action(&mut entry.window);
        entry.window.write_into(&mut bag);
        entry.write_view(&bag);
        found = true;
    }

    found
}

/// Holds the visible row `visible_row` of the bag at `offset` in place.
pub fn pin_row(offset: usize, visible_row: usize) -> bool {
    with_offset(offset, |window| window.pin_row(visible_row))
}

/// Releases the held row of the bag at `offset`, keeping visible slot `keep`
/// showing what it shows now where possible.
pub fn unpin(offset: usize, keep: Option<usize>) -> bool {
    with_offset(offset, |window| window.unpin(keep))
}

/// The item showing in visible slot `slot` of the bag at `offset`.
pub fn item_in_view(offset: usize, slot: usize) -> Option<Item> {
    let registry = REGISTRY.lock().ok()?;

    let entry = registry.entries.iter().find(|e| e.offset == offset)?;
    let index = entry.window.store_index(slot)?;
    entry.window.store().get(index)
}

/// Brings every store's window back onto its equipped item, if it left it.
pub fn keep_equipped_visible_all() {
    if let Ok(mut registry) = REGISTRY.lock() {
        for entry in registry.entries.iter_mut() {
            entry.sync_for_the_world();
        }
    }
}

/// Where each store's window sits, and what it is showing, for logging.
pub fn positions() -> Vec<(usize, usize, usize, usize)> {
    match REGISTRY.lock() {
        Ok(registry) => registry
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.owner + entry.offset,
                    entry.window.position(),
                    entry.window.store().capacity(),
                    entry.window.store().count_empty(),
                )
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

// --- Surviving a save and a reload ---

/// Every store as it stands, for writing to the side file.
pub fn snapshot() -> Vec<(usize, usize, Vec<Item>)> {
    let Ok(mut registry) = REGISTRY.lock() else {
        return Vec::new();
    };

    registry
        .entries
        .iter_mut()
        .map(|entry| {
            // Take in whatever the game has written since the last accessor
            // call, so what goes into the file is what the player has now.
            let bag = entry.read_view();
            entry.window.read_from(&bag);

            (
                entry.offset,
                entry.window.position(),
                entry.window.store().as_slice().to_vec(),
            )
        })
        .collect()
}

/// What is waiting to be put back once the game has finished loading.
///
/// A load cannot be answered on the spot. The game copies the save into its own
/// bags after the moment this mod hears about it, and whatever this mod wrote
/// first would be overwritten. So the restore waits here, and is applied on the
/// far side of the reseed that copy triggers.
static PENDING: Mutex<Vec<Staged>> = Mutex::new(Vec::new());

/// Whether `PENDING` might hold anything, so the accessor path can skip the
/// lock entirely. The accessor runs at least once per drawn frame for the whole
/// session, and something is staged for seconds after a load at most.
static PENDING_ANY: AtomicBool = AtomicBool::new(false);

/// Set when a staged restore has matched the game's bag since the last load.
/// The save module reads this to decide the side file belongs to this save.
static MATCHED: AtomicBool = AtomicBool::new(false);

/// How long a staged restore waits for the game's bag to match it.
///
/// The gap being bridged is the game copying the save into its bags, which
/// takes a loading screen. A record that has not matched by the end of this is
/// from some other save, and holding it longer only gives it chances to match
/// the live inventory by coincidence and overwrite it with stale items.
const STAGED_LIFETIME: Duration = Duration::from_secs(60);

pub struct Restore {
    pub offset: usize,
    pub position: usize,
    pub items: Vec<Item>,
    /// The six the game should have restored, if this belongs to that save.
    pub expected_visible: Vec<Item>,
}

/// One staged restore, and what has happened to it so far.
struct Staged {
    restore: Restore,
    since: Instant,
    /// Applied once already. Kept rather than removed: an application before
    /// the game finished copying the save in is overwritten by the reseed that
    /// copy causes, and the answer to that is applying again, not having
    /// consumed the record.
    applied: bool,
}

/// Holds a restore until the game has loaded.
pub fn stage(restores: Vec<Restore>) {
    if let Ok(mut pending) = PENDING.lock() {
        log_info!("{} store(s) staged for restoring after the load.", restores.len());
        MATCHED.store(false, Ordering::Relaxed);
        *pending = restores
            .into_iter()
            .map(|restore| Staged {
                restore,
                since: Instant::now(),
                applied: false,
            })
            .collect();
        PENDING_ANY.store(!pending.is_empty(), Ordering::Release);
    }
}

/// Drops anything staged, for a load that turned out not to happen.
pub fn discard_staged() {
    if let Ok(mut pending) = PENDING.lock() {
        pending.clear();
        PENDING_ANY.store(false, Ordering::Release);
        MATCHED.store(false, Ordering::Relaxed);
    }
}

/// Whether a staged restore has matched the loaded save yet.
///
/// This is the signal that the side file describes the save actually loaded.
/// The box's contents ride on it: they have no bag in the game to be verified
/// against, so they are trusted exactly as far as the bags that do.
pub fn restore_matched() -> bool {
    MATCHED.load(Ordering::Relaxed)
}

/// Puts a staged store back, if this bag is one that was waiting for it.
///
/// Called from `view_for` on every pass while something is staged. It widens
/// the store back to everything the side file recorded, but only once the six
/// slots the game's bag holds agree with the six recorded alongside — that is
/// what says the game has actually finished copying this save in.
///
/// Three deliberate choices, each covering a failure that was found by review
/// rather than in play:
///
/// - Every candidate at this offset is tried, not just the first. A file can
///   hold two records for `+0x20`, and the stale one must not shadow the live
///   one.
/// - A restore that matches is applied but kept. The match can happen *before*
///   the game copies the save in — the load hook runs early, and six empty
///   slots match six empty slots — and the copy then reseeds the store,
///   undoing the application. The kept record matches again afterwards, and
///   the second application is the one that lasts.
/// - Everything staged expires after `STAGED_LIFETIME`. A record from some
///   other save never matches during the load, but the live inventory drifts,
///   and given forever it would eventually coincide — and be overwritten with
///   stale items mid-session.
fn apply_staged(entry: &mut Entry, restored: &Bag) {
    if !PENDING_ANY.load(Ordering::Acquire) {
        return;
    }

    let Ok(mut pending) = PENDING.lock() else {
        return;
    };

    pending.retain(|staged| {
        let keep = staged.since.elapsed() < STAGED_LIFETIME;

        if !keep && !staged.applied {
            log_warn!(
                "The side record for bag +0x{:02X} never matched the loaded save; \
                 its {} item(s) were not restored.",
                staged.restore.offset,
                staged.restore.items.len()
            );
        }

        keep
    });

    for staged in pending
        .iter_mut()
        .filter(|staged| staged.restore.offset == entry.offset)
    {
        if staged.restore.expected_visible[..] != restored.items[..] {
            continue;
        }

        let configured = entry.window.store().capacity();
        let restore = &staged.restore;

        if restore.items.len() > configured {
            log_warn!(
                "The record for bag +0x{:02X} holds {} items but {} slots are configured; \
                 keeping every item.",
                restore.offset,
                restore.items.len(),
                configured
            );
        }

        entry.window = Window::with_items(configured, &restore.items, restore.position);

        if !staged.applied {
            staged.applied = true;
            log_info!(
                "Bag +0x{:02X} restored to {} slots from the side file.",
                restore.offset,
                entry.window.store().capacity()
            );
        }

        MATCHED.store(true, Ordering::Relaxed);
        break;
    }

    PENDING_ANY.store(!pending.is_empty(), Ordering::Release);
}

/// The items a store holds, for logging.
pub fn contents(index: usize) -> Vec<Item> {
    match REGISTRY.lock() {
        Ok(registry) => registry
            .entries
            .get(index)
            .map(|entry| entry.window.store().as_slice().to_vec())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Runs `action` against the store behind a view the game was given.
///
/// Returns `None` when the pointer is not one of ours, which happens whenever
/// the game reached a bag by a route this mod does not intercept. The caller
/// then falls back to the bag itself rather than guessing.
pub fn with_view<R>(view: *const Bag, action: impl FnOnce(&mut Window) -> R) -> Option<R> {
    let mut registry = REGISTRY.lock().ok()?;

    let index = registry
        .entries
        .iter()
        .position(|entry| std::ptr::eq(entry.view as *const Bag, view))?;

    let entry = &mut registry.entries[index];

    let mut bag = entry.read_view();
    entry.window.read_from(&bag);
    let result = action(&mut entry.window);
    entry.window.write_into(&mut bag);
    entry.write_view(&bag);

    Some(result)
}

/// Slots the game can see at once, re-exported so callers do not reach past
/// this module for it.
pub const VISIBLE_SLOTS: usize = BAG_SIZE;

/// Shows the window at each position in turn until `ask` is satisfied.
///
/// This exists because the game answers "is this item in the bag" by walking
/// six slots, and six slots is all it can be shown. Rather than reimplement
/// what counts as a match — which covers item types, the personal slot and
/// several special ids — the window is moved and the game's own answer is
/// taken. Slower on a miss, and exactly as correct as the original.
///
/// The window is left wherever the answer was found, deliberately: the answer
/// is a slot number, and the caller is about to use it to reach into the six
/// slots it can see. Putting the window back would invalidate the very number
/// just returned.
///
/// Returns `None` when the pointer is not one of ours, or when no position
/// satisfied `ask`; the window is then back where it started.
pub fn probe_positions(view: *mut Bag, mut ask: impl FnMut() -> i32) -> Option<i32> {
    let mut registry = REGISTRY.lock().ok()?;

    let index = registry
        .entries
        .iter()
        .position(|entry| std::ptr::eq(entry.view as *const Bag, view))?;

    let entry = &mut registry.entries[index];

    let mut bag = entry.read_view();
    entry.window.read_from(&bag);

    let started_at = entry.window.position();
    let candidates: Vec<usize> = entry.window.positions().collect();

    for position in candidates {
        entry.window.set_position(position);
        entry.window.write_into(&mut bag);
        entry.write_view(&bag);

        // The game reads the view we just published. Holding the lock across
        // it is safe because nothing it runs is hooked: `Bag::find_item` at
        // 0x004DB130 calls only the item-kind lookup 0x004DD4D0, its two
        // searches 0x004DBD60 and 0x004DBDB0, the assert, and the leaf
        // 0x0059B8D0. See docs/game-internals.md, "Searching the bag".
        let answer = ask();

        if answer >= 0 {
            return Some(answer);
        }

        // Whatever the game left in the view during the search is its business
        // and not a change to record, so the bag is re-read rather than kept.
        bag = entry.read_view();
    }

    entry.window.set_position(started_at);
    entry.window.write_into(&mut bag);
    entry.write_view(&bag);

    None
}

/// Sends every window back to the first slot.
///
/// Used to wrap around at the end of the list. Scrolling only downwards keeps
/// the binding out of the way of what the game already does with "up" on the
/// top row, which is to move to the tabs above the panel.
pub fn rewind_all() -> usize {
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };

    let mut moved = 0;

    for entry in registry.entries.iter_mut() {
        let mut bag = entry.read_view();
        entry.window.read_from(&bag);

        if entry.window.position() == 0 {
            continue;
        }

        entry.window.reset();
        entry.window.write_into(&mut bag);
        entry.write_view(&bag);
        moved += 1;
    }

    moved
}
