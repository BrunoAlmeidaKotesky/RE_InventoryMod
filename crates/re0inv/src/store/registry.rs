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

use std::sync::Mutex;

use crate::core::logging::{log_debug, log_info};
use crate::game::inventory::{Bag, Item, BAG_SIZE};
use crate::store::window::Window;

/// One character's storage, and the bag the game is shown in its place.
struct Entry {
    /// The object holding the game's own bag, and the offset within it. Together
    /// these identify a character's inventory across frames, which a heap
    /// address on its own does not.
    owner: usize,
    offset: usize,

    /// The bag handed to the game. Boxed so its address never moves: the game
    /// keeps hold of it between calls.
    view: Box<Bag>,

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
    /// Refreshes the view from the store, after taking in whatever the game
    /// wrote into it since last time.
    fn sync(&mut self) {
        self.window.read_from(&self.view);
        self.window.write_into(&mut self.view);
    }

    /// Starts the store over from a bag the game filled in behind our back.
    fn reseed(&mut self, source: &Bag) {
        self.window = Window::new(self.window.store().capacity());
        self.window.read_from(source);
        *self.view = *source;
    }

    /// Reports what the game is about to be shown, when it changes.
    ///
    /// The accessor runs once per drawn frame, so this only speaks up when the
    /// answer is different from last time. Without that it would be thousands of
    /// identical lines; with it, every line is a change worth explaining.
    fn report(&mut self) {
        let ids: Vec<i32> = self.view.items.iter().map(|item| item.id).collect();
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
            self.view.equipped_index
        );
    }
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

    let own = &mut *((owner + offset) as *mut Bag);

    let index = match registry.find(owner, offset) {
        Some(index) => index,
        None => {
            let capacity = registry.capacity;

            let mut window = Window::new(capacity);
            window.read_from(own);

            log_info!(
                "New store for the bag at 0x{:08X}+0x{:02X}: {} slots.",
                owner,
                offset,
                window.store().capacity()
            );

            registry.entries.push(Entry {
                owner,
                offset,
                view: Box::new(*own),
                mirror: *own,
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
    if entry.mirror != *own {
        log_info!("Bag at 0x{:08X}+0x{:02X} changed underneath us; reseeding.", owner, offset);
        entry.reseed(own);
    }

    entry.sync();

    // Keep the game's own bag showing what the view shows. Code that reaches it
    // without an accessor then sees the visible slots rather than a stale copy,
    // and this is also what makes the comparison above mean anything.
    *own = *entry.view;
    entry.mirror = *own;

    entry.report();

    &mut *entry.view as *mut Bag
}

/// Forgets every store.
///
/// The object a store is keyed on is freed and rebuilt on a new game or a load.
/// Keeping one keyed on a stale address would hand a character's items to
/// whatever is allocated there next.
pub fn forget_all() {
    if let Ok(mut registry) = REGISTRY.lock() {
        let count = registry.entries.len();
        registry.entries.clear();
        log_info!("Dropped {count} store(s).");
    }
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
        entry.window.read_from(&entry.view);

        if !entry.window.scroll_rows(rows) {
            continue;
        }

        entry.window.write_into(&mut entry.view);
        moved += 1;
    }

    moved
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
        .position(|entry| std::ptr::eq(&*entry.view as *const Bag, view))?;

    let entry = &mut registry.entries[index];

    entry.window.read_from(&entry.view);
    let result = action(&mut entry.window);
    entry.window.write_into(&mut entry.view);

    Some(result)
}

/// Slots the game can see at once, re-exported so callers do not reach past
/// this module for it.
pub const VISIBLE_SLOTS: usize = BAG_SIZE;

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
        entry.window.read_from(&entry.view);

        if entry.window.position() == 0 {
            continue;
        }

        entry.window.reset();
        entry.window.write_into(&mut entry.view);
        moved += 1;
    }

    moved
}
