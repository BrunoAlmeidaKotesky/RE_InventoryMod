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

    let open = !OPEN.fetch_xor(true, Ordering::Relaxed);
    log_info!(
        "Partner panel now showing {}.",
        if open { "the item box" } else { "the partner's bag" }
    );
    open
}

/// Hides the box.
///
/// Called whenever the inventory is not open. The accessor the box borrows
/// answers questions outside the menu too, and those must never be answered
/// with the box's contents.
pub fn close() {
    if OPEN.swap(false, Ordering::Relaxed) {
        log_info!("Item box closed.");
    }
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
