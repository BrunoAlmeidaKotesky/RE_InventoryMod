//! Storage the player can put things into and take things back out of.
//!
//! # Where it appears
//!
//! In the partner's half of the inventory screen, in place of the partner's own
//! bag, toggled by a key. The panel is already a generic surface fed by an
//! accessor this mod replaces, so showing something else there costs one branch
//! and no game assets at all.
//!
//! The alternative was a new option on the typewriter menu, which is where the
//! series usually puts a box. That needs a new line of on-screen text, and text
//! lives in Capcom's message archives, so it would mean shipping modified game
//! files — in every language, or in none.
//!
//! What this costs instead is discoverability: there is no label saying the key
//! exists. That is a real loss and there is no way to dress it up.
//!
//! # Moving items is free
//!
//! Passing an item between the two halves of the screen is something the game
//! already does. With the box in the right-hand panel, handing an item to the
//! partner is depositing it, and taking one back is withdrawing. No transfer
//! logic is written here.
//!
//! # Why it may only be open inside the menu
//!
//! The accessor being borrowed has callers outside the inventory screen —
//! "does the partner have this item" among them. While the box is showing,
//! those would be asking the box, and getting the wrong answer. So the flag is
//! only ever set while the inventory is open, and cleared before anything
//! returns to the world.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::logging::{log_info, log_warn};
use crate::game::inventory::{Bag, Item, BAG_SIZE};
use crate::store::window::Window;

/// Whether the partner panel is currently showing the box.
static OPEN: AtomicBool = AtomicBool::new(false);

static STORAGE: Mutex<Option<Storage>> = Mutex::new(None);

struct Storage {
    /// The bag handed to the game, as a raw allocation that is never freed and
    /// never turned into a reference. Same reasoning as the inventory views:
    /// the game writes into it, and a reference would be a promise this cannot
    /// keep.
    view: *mut Bag,
    window: Window,
}

// Safety: the pointer is an allocation reached only through volatile access,
// and it is never freed.
unsafe impl Send for Storage {}

impl Storage {
    fn new(capacity: usize) -> Storage {
        let empty = Bag {
            unknown00: 0,
            items: [Item::EMPTY; BAG_SIZE],
            personal_item: Item::EMPTY,
            equipped_index: -1,
        };

        Storage {
            view: Box::into_raw(Box::new(empty)),
            window: Window::new(capacity),
        }
    }

    fn read(&self) -> Bag {
        unsafe { self.view.read_volatile() }
    }

    fn write(&self, bag: &Bag) {
        unsafe { self.view.write_volatile(*bag) };
    }

    /// Takes in whatever the game left in the view, then publishes the window.
    fn sync(&mut self) {
        let mut bag = self.read();
        self.window.read_from(&bag);
        self.window.write_into(&mut bag);
        self.write(&bag);
    }
}

/// Whether the box is showing in place of the partner's bag.
pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// Shows the box, or puts the partner's bag back.
///
/// Returns the new state. Refuses to open if the box was never set up, which
/// happens when the feature is switched off.
pub fn toggle() -> bool {
    if !is_open() && !exists() {
        log_warn!("The item box is switched off in the configuration.");
        return false;
    }

    // The box lives at the typewriter, as it does in the rest of the series.
    // Away from one it cannot be opened at all.
    if !is_open() && !within_reach() {
        log_info!("The item box is only reachable at a typewriter.");
        return false;
    }

    let open = !OPEN.fetch_xor(true, Ordering::Relaxed);
    log_info!(
        "Partner panel now showing {}.",
        if open { "the item box" } else { "the partner's bag" }
    );
    open
}

/// Shows the box because the player asked for it on the typewriter prompt.
///
/// No reachability check: the request came from the typewriter itself, which is
/// a better proof of standing at one than any timer.
pub fn force_open() {
    if !exists() {
        log_warn!("The item box was chosen, but it is switched off in the configuration.");
        return;
    }

    if !OPEN.swap(true, Ordering::Relaxed) {
        log_info!("Item box opened from the typewriter.");
    }
}

/// Puts the partner's bag back when the inventory screen closes.
///
/// The box lives for exactly one visit to the screen, which is what the player
/// sees: choose it at the typewriter, move things, back out, and the partner's
/// bag is there again next time.
///
/// Two earlier rules were worse. Closing when the selection had not moved for a
/// while closed the box the instant it opened, because at a typewriter the
/// cursor never moves. Closing when the machine went out of reach closed it
/// mid-use, because the prompt stops running once the screen is up.
pub fn close_with_menu() {
    if !OPEN.swap(false, Ordering::Relaxed) {
        return;
    }

    // Safety: the screen has only just closed, so the object is still there.
    // Leaving exchanging forced open would let the player hand items to a
    // partner who is nowhere near.
    unsafe { crate::hook::panel::restore_partner_half() };

    log_info!("Inventory closed; the partner's bag is back.");
}

/// Sets the box up with room for `capacity` items.
pub fn enable(capacity: usize) {
    if let Ok(mut storage) = STORAGE.lock() {
        *storage = Some(Storage::new(capacity));
        log_info!("Item box enabled with {capacity} slots.");
    }
}

fn exists() -> bool {
    STORAGE.lock().map(|s| s.is_some()).unwrap_or(false)
}

/// The bag to hand the game while the box is showing.
///
/// Returns null when there is no box, which leaves the caller to fall back on
/// the partner's real bag.
pub fn view() -> *mut Bag {
    let Ok(mut storage) = STORAGE.lock() else {
        return std::ptr::null_mut();
    };

    let Some(storage) = storage.as_mut() else {
        return std::ptr::null_mut();
    };

    storage.sync();
    storage.view
}

/// Runs `action` against the box's window, if `view` is the box's own.
///
/// The box keeps its storage here rather than in the inventory registry, so
/// everything that works off the registry — how many slots are free, where the
/// next free one is, searching for an item — misses it and falls back to the
/// six the game can see. Which is how a twenty-four slot box came to report
/// itself full after six items.
pub fn with_window<R>(view: *const Bag, action: impl FnOnce(&mut Window) -> R) -> Option<R> {
    let mut storage = STORAGE.lock().ok()?;
    let storage = storage.as_mut()?;

    if !std::ptr::eq(storage.view as *const Bag, view) {
        return None;
    }

    let mut bag = storage.read();
    storage.window.read_from(&bag);
    let result = action(&mut storage.window);
    storage.window.write_into(&mut bag);
    storage.write(&bag);

    Some(result)
}

/// Shows the box at each window position in turn until `ask` is satisfied.
///
/// The same trick the inventory uses to answer "is this item in the bag" for
/// slots outside the window, and needed here for the same reason: the game
/// walks six slots and the box has many more.
pub fn probe_positions(view: *const Bag, mut ask: impl FnMut() -> i32) -> Option<i32> {
    let mut storage = STORAGE.lock().ok()?;
    let storage = storage.as_mut()?;

    if !std::ptr::eq(storage.view as *const Bag, view) {
        return None;
    }

    let mut bag = storage.read();
    storage.window.read_from(&bag);

    let started_at = storage.window.position();
    let candidates: Vec<usize> = storage.window.positions().collect();

    for position in candidates {
        storage.window.set_position(position);
        storage.window.write_into(&mut bag);
        storage.write(&bag);

        let answer = ask();
        if answer >= 0 {
            return Some(answer);
        }

        bag = storage.read();
    }

    storage.window.set_position(started_at);
    storage.window.write_into(&mut bag);
    storage.write(&bag);

    None
}

/// Moves the box's window by whole rows, and reports whether it moved.
pub fn scroll(rows: i32) -> bool {
    let Ok(mut storage) = STORAGE.lock() else {
        return false;
    };

    let Some(storage) = storage.as_mut() else {
        return false;
    };

    let mut bag = storage.read();
    storage.window.read_from(&bag);

    if !storage.window.scroll_rows(rows) {
        return false;
    }

    storage.window.write_into(&mut bag);
    storage.write(&bag);
    true
}

/// Where the window sits and how much of the box is free, for logging.
pub fn state() -> Option<(usize, usize, usize)> {
    let storage = STORAGE.lock().ok()?;
    let storage = storage.as_ref()?;

    Some((
        storage.window.position(),
        storage.window.store().capacity(),
        storage.window.store().count_empty(),
    ))
}

// --- Only at a typewriter ---

use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

/// How long after last standing at a typewriter the box stays reachable.
///
/// Generous on purpose. The routine below runs while the player is at the
/// machine, and the box is then opened from the inventory screen, which takes a
/// moment to get to. Being strict here would mean walking back and forth.
const REACH: Duration = Duration::from_secs(20);

static LAST_TYPEWRITER: Mutex<Option<Instant>> = Mutex::new(None);
static TYPEWRITER_CONTINUE: AtomicUsize = AtomicUsize::new(0);

pub fn set_typewriter_continue(address: usize) {
    TYPEWRITER_CONTINUE.store(address, Ordering::Relaxed);
}

/// Whether the player is at, or has just been at, a typewriter.
pub fn within_reach() -> bool {
    LAST_TYPEWRITER
        .lock()
        .ok()
        .and_then(|at| *at)
        .is_some_and(|at| at.elapsed() < REACH)
}

/// Trampoline over the start of the typewriter routine.
///
/// The box belongs at the typewriter, the way the series has always done it.
/// Putting an option on its menu is what re0box does, and that needs a new line
/// of on-screen text: the code here only reads which choice came back, while the
/// number of choices comes from the message data inside Capcom's archives. So
/// there is no code-only way to add one.
///
/// Knowing the player is *at* the machine needs no new text at all, and this is
/// how that is known.
///
/// # Safety
/// Reached only through the detour written over the first three instructions of
/// that routine. Those are re-executed here before control returns.
#[unsafe(naked)]
pub unsafe extern "C" fn typewriter_stub() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "call {observe}",
        "mov esp, ebp",
        "popad",
        "popfd",
        // The three instructions this replaced.
        "sub esp, 0x54",
        "push esi",
        "push edi",
        "jmp dword ptr [{continue_at}]",
        observe = sym note_typewriter,
        continue_at = sym TYPEWRITER_CONTINUE,
    )
}

extern "C" fn note_typewriter() {
    let _ = std::panic::catch_unwind(|| {
        if let Ok(mut at) = LAST_TYPEWRITER.lock() {
            let first = at.is_none();
            *at = Some(Instant::now());

            if first {
                log_info!("Typewriter reached; the item box is available from the inventory.");
            }
        }
    });
}
