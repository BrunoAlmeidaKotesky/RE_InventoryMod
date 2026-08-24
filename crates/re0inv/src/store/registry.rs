//! Which store belongs to which of the game's bags.
//!
//! The game keeps two bags alive at once, adjacent fields of one object at
//! `+0x20` and `+0x60`, and hands whichever one it means to each method call.
//! So a store has to be found by the bag it stands behind.
//!
//! # Keeping the two in step
//!
//! The game writes into the bag directly, at sites this mod does not intercept.
//! Rather than trying to hook every write, every intercepted call starts by
//! reading the visible slots back into the store, and ends by writing the
//! visible slots out again.
//!
//! That works because the window only moves while we hold it: whatever the game
//! wrote at visible slot `k` is still at store index `position + k` the next
//! time we look.

use std::sync::Mutex;

use crate::core::logging::{log_debug, log_info};
use crate::game::inventory::{Bag, BAG_BYTES};
use crate::store::window::Window;

/// A bag the game owns, and the storage standing behind it.
struct Entry {
    /// Address of the game's bag. Bags live inside a heap object, so this is
    /// only meaningful for as long as that object does.
    bag: usize,
    window: Window,
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

    /// Finds the store for a bag, creating one seeded from the bag's current
    /// contents the first time that bag is seen.
    fn window_for(&mut self, bag: *const Bag) -> &mut Window {
        let address = bag as usize;

        if let Some(position) = self.entries.iter().position(|e| e.bag == address) {
            return &mut self.entries[position].window;
        }

        let mut window = Window::new(self.capacity);

        // Seed from what the game already has there, so a store that appears
        // mid-game inherits the inventory instead of emptying it.
        window.read_from(unsafe { &*bag });

        log_info!(
            "New store for the bag at 0x{:08X}: {} slots.",
            address,
            window.store().capacity()
        );

        self.entries.push(Entry { bag: address, window });
        &mut self.entries.last_mut().expect("just pushed").window
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

/// Runs `action` against the store behind `bag`, syncing both ways around it.
///
/// Returns `None` if the registry is unusable, which leaves the caller to fall
/// back on the bag itself rather than guess.
///
/// # Safety
/// `bag` must point at a readable, writable `Bag` for the duration of the call.
pub unsafe fn with_store<R>(bag: *mut Bag, action: impl FnOnce(&mut Window) -> R) -> Option<R> {
    // A poisoned registry means a previous call panicked while holding it. The
    // store's contents cannot be trusted after that, so stop using it rather
    // than hand the game answers derived from it.
    let mut registry = REGISTRY.lock().ok()?;

    let window = registry.window_for(bag);

    // Pull in anything the game wrote since we last looked.
    window.read_from(&*bag);

    let result = action(window);

    // Push back out: the action may have moved the window.
    window.write_into(&mut *bag);

    Some(result)
}

/// Forgets every store.
///
/// The bags a store is keyed on live in a heap object that the game frees and
/// rebuilds, on a new game or a load. Keeping a store keyed on a stale address
/// would hand one character's items to whatever is allocated there next.
pub fn forget_all() {
    if let Ok(mut registry) = REGISTRY.lock() {
        let count = registry.entries.len();
        registry.entries.clear();
        log_info!("Dropped {count} store(s).");
    }
}

/// Scrolls every store by `rows` rows of two, and rewrites the bags.
///
/// Every store moves together rather than only the panel being looked at. The
/// panels sit side by side, so moving both shows the same rows of each, and it
/// sidesteps a question the game has not answered yet: which character a given
/// panel belongs to. Getting that wrong would scroll the other character's
/// inventory, which is worse than scrolling both.
///
/// The bag is written here rather than waiting for the next intercepted call,
/// because the menu draws from the bag and nothing else would happen until the
/// player did something that happens to be hooked.
///
/// Returns how many stores actually moved.
pub fn scroll_all(rows: i32) -> usize {
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };

    let mut moved = 0;

    for entry in registry.entries.iter_mut() {
        // The bag lives in an object the game owns and can free. Reading it
        // first turns a stale address into a skipped entry instead of a crash.
        if crate::debug::memory::read_array::<BAG_BYTES>(entry.bag).is_none() {
            log_debug!("Bag at 0x{:08X} is gone; skipping.", entry.bag);
            continue;
        }

        let bag = entry.bag as *mut Bag;

        // Take in whatever the game wrote before moving, or the slots currently
        // on screen would be lost.
        unsafe { entry.window.read_from(&*bag) };

        if !entry.window.scroll_rows(rows) {
            continue;
        }

        unsafe { entry.window.write_into(&mut *bag) };
        moved += 1;
    }

    moved
}

/// Where each store's window currently sits, for logging.
pub fn positions() -> Vec<(usize, usize, usize)> {
    match REGISTRY.lock() {
        Ok(registry) => registry
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.bag,
                    entry.window.position(),
                    entry.window.store().capacity(),
                )
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Addresses of every bag a store has been created for.
///
/// Used by the menu probe: knowing which addresses are bags turns "dump some
/// memory and squint" into "find where the menu keeps the bag it is showing".
pub fn known_bags() -> Vec<usize> {
    match REGISTRY.lock() {
        Ok(registry) => registry.entries.iter().map(|entry| entry.bag).collect(),
        Err(_) => Vec::new(),
    }
}
