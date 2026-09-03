//! Replacements for the game's `Bag` methods.
//!
//! # Calling convention
//!
//! These are C++ methods compiled by MSVC, so they are `__thiscall`: the object
//! pointer arrives in `ecx` rather than on the stack, and the callee cleans up
//! whatever stack arguments there are.
//!
//! Rust cannot declare `__thiscall` on stable, so each replacement is a pair: a
//! naked stub that matches the game's convention exactly and does nothing but
//! move `ecx` onto the stack, and an ordinary function that does the work.
//!
//! Getting this wrong does not fail loudly. It unbalances the caller's stack,
//! and the crash lands somewhere unrelated, long after the real mistake.
//!
//! # Answering for a store the game cannot see
//!
//! These methods answer questions about a six-slot bag, and the game acts on
//! the answers inside that bag. So a count may never promise more than one
//! window position can show, and an index may never name a slot the game
//! cannot address, however many slots the store behind it has. Where the store
//! has the room but the window does not show it, the window moves first.
//!
//! `Bag::add_item` (`0x004DB4C0`) is what enforces this. For a two-slot item
//! it checks `count_empty` against two, writes the item at `first_empty`, and
//! has `organize` lay the pair out, asserting every index it touches against
//! six. Told three with one slot in view, it placed the item in slot 5,
//! reached for slot 6, and the game's own assertion ended the process.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::feature::item_box;
use crate::game::inventory::{Bag, BAG_SIZE};
use crate::store::registry;
use crate::store::window::Window;

/// Calls logged per replacement before it goes quiet.
///
/// These run from menu code and can fire every frame. Enough lines to prove the
/// hook is live and returning sane values, then silence.
const LOG_LIMIT: usize = 8;

/// What the game's own search returns when there is no empty slot.
const NO_EMPTY_SLOT: i32 = -1;

static COUNT_EMPTY_CALLS: AtomicUsize = AtomicUsize::new(0);
static FIRST_EMPTY_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Return addresses of the calls to `first_empty` made for a two-slot item.
/// Filled in at install time from the address table.
static TWO_SLOT_CALLERS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

pub fn set_two_slot_callers(callers: [usize; 2]) {
    for (slot, address) in TWO_SLOT_CALLERS.iter().zip(callers) {
        slot.store(address, Ordering::Relaxed);
    }
}

fn is_two_slot_request(called_from: usize) -> bool {
    called_from != 0
        && TWO_SLOT_CALLERS
            .iter()
            .any(|caller| caller.load(Ordering::Relaxed) == called_from)
}

/// Logs the first few calls to a replacement, then stops.
fn should_log(calls: &AtomicUsize, name: &str) -> bool {
    let seen = calls.fetch_add(1, Ordering::Relaxed);

    if seen == LOG_LIMIT {
        log_debug!("{name}: logged {LOG_LIMIT} calls, staying quiet from here.");
    }

    seen < LOG_LIMIT
}

/// Checks the pointer the game put in `ecx` as far as is possible from here.
fn is_usable(bag: *const Bag) -> bool {
    // A misaligned or null pointer means we are not being called as a method at
    // all, which is worth knowing before dereferencing it.
    !bag.is_null() && (bag as usize).is_multiple_of(4)
}

/// Entry stub for the game's "how many empty slots" method.
///
/// # Safety
/// Only reachable through the detour installed over the game's function, which
/// guarantees the convention: `ecx` holds the bag, no stack arguments, and the
/// caller expects the result in `eax`.
#[unsafe(naked)]
pub unsafe extern "C" fn count_empty_stub() {
    core::arch::naked_asm!(
        // Move `this` from ecx to the stack, where a C function expects it.
        "push ecx",
        "call {handler}",
        // cdecl leaves the argument for the caller to remove.
        "add esp, 4",
        "ret",
        handler = sym count_empty,
    )
}

/// How many empty slots the character has, as far as the game may be told.
///
/// Counted across the store, not just the six in view, which is what makes a
/// larger inventory real; but capped at what one window position can show,
/// because that is where the game will put whatever it was promised room for.
extern "C" fn count_empty(bag: *mut Bag) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        if !is_usable(bag) {
            log_warn!("count_empty called with an unusable pointer: {:?}", bag);
            return 0;
        }

        let count = |window: &mut Window| promised_room(window) as i32;

        // The box keeps its storage outside the registry, so it has to be asked
        // separately. Without this a twenty-four slot box reports itself full
        // after six items, because six is all the game can see of it.
        let empty = registry::with_view(bag, count)
            .or_else(|| item_box::with_window(bag, count))
            .unwrap_or_else(|| visible_empty_count(&*bag));

        if should_log(&COUNT_EMPTY_CALLS, "count_empty") {
            log_info!(
                "count_empty(0x{:08X}) = {}  [{}]",
                bag as usize,
                empty,
                describe(&*bag)
            );
        }

        empty
    });

    // A panic must not unwind into the game. Returning zero says "the bag is
    // full", which makes the game refuse to add an item: wrong, but harmless.
    result.unwrap_or_else(|_| {
        log_warn!("count_empty panicked.");
        0
    })
}

/// Entry stub for the game's "index of the first empty slot" method.
///
/// # Safety
/// Same contract as `count_empty_stub`, plus: the return address is read off
/// the stack before anything is pushed, so the handler knows which caller is
/// asking.
#[unsafe(naked)]
pub unsafe extern "C" fn first_empty_stub() {
    core::arch::naked_asm!(
        "push dword ptr [esp]",
        "push ecx",
        "call {handler}",
        "add esp, 8",
        "ret",
        handler = sym first_empty,
    )
}

/// A slot the caller may write an item into, or `-1` if there is none.
///
/// The returned index is used directly as `bag->items[index]`, so it has to be
/// one of the six the game has. Which caller is asking decides how the slot is
/// chosen; see `seek_slot`.
extern "C" fn first_empty(bag: *mut Bag, called_from: usize) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        if !is_usable(bag) {
            log_warn!("first_empty called with an unusable pointer: {:?}", bag);
            return NO_EMPTY_SLOT;
        }

        let two_slot = is_two_slot_request(called_from);
        let seek = |window: &mut Window| seek_slot(window, two_slot);

        let found = registry::with_view(bag, seek)
            .or_else(|| item_box::with_window(bag, seek))
            .unwrap_or_else(|| visible_first_empty(&*bag));

        if should_log(&FIRST_EMPTY_CALLS, "first_empty") {
            log_info!(
                "first_empty(0x{:08X}) = {}  [{}]{}",
                bag as usize,
                found,
                describe(&*bag),
                if two_slot { "  two-slot" } else { "" }
            );
        }

        found
    });

    // "No empty slot" is the conservative answer: the game declines to add the
    // item rather than writing one somewhere it should not.
    result.unwrap_or_else(|_| {
        log_warn!("first_empty panicked.");
        NO_EMPTY_SLOT
    })
}

/// The empty slots the game may be promised: what one window position shows.
fn promised_room(window: &Window) -> usize {
    window.roomiest_position().1
}

/// The visible slot a caller may write into, moving the window if needed.
///
/// A single item goes to the first empty slot in view. When none is in view
/// but the store has room, the window jumps to it: the game's own answer,
/// extended past the six.
///
/// A two-slot item was just promised `promised_room` slots, and the game will
/// write it here and have `organize` fit the pair among the six in view. So
/// the window moves to the position that promise was counted at, and the
/// answer is the first empty slot there. Any other answer can name a slot
/// whose neighbour is outside the bag.
fn seek_slot(window: &mut Window, two_slot: bool) -> i32 {
    if two_slot {
        let (position, room) = window.roomiest_position();
        if room == 0 {
            return NO_EMPTY_SLOT;
        }

        window.set_position(position);
        return window
            .first_visible_empty()
            .map_or(NO_EMPTY_SLOT, |slot| slot as i32);
    }

    if let Some(slot) = window.first_visible_empty() {
        return slot as i32;
    }

    let Some(index) = window.store().first_empty() else {
        return NO_EMPTY_SLOT;
    };

    window.reveal(index);
    window
        .visible_slot(index)
        .map_or(NO_EMPTY_SLOT, |slot| slot as i32)
}

/// What the game's own code would have answered, ignoring the store.
///
/// Used when the store is unavailable. Falling back to the bag keeps the game
/// correct-but-unmodded instead of feeding it an answer derived from something
/// we could not read.
fn visible_empty_count(bag: &Bag) -> i32 {
    bag.items.iter().filter(|item| item.is_empty()).count() as i32
}

fn visible_first_empty(bag: &Bag) -> i32 {
    bag.items
        .iter()
        .position(|item| item.is_empty())
        .map_or(NO_EMPTY_SLOT, |index| index as i32)
}

/// The bag's item ids, for the log.
fn describe(bag: &Bag) -> String {
    let mut text = String::with_capacity(BAG_SIZE * 4);

    for (i, item) in bag.items.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&item.id.to_string());
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::inventory::Item;

    const HERB: i32 = 30;

    fn herb() -> Item {
        Item { id: HERB, count: 1 }
    }

    /// Nine items in twelve slots, window on slots 4..10: one empty slot in
    /// view, three in the store. The state that took the game down.
    fn nine_of_twelve() -> Window {
        let mut window = Window::new(12);
        for i in 0..9 {
            window.store_mut().set(i, herb());
        }
        window.set_position(4);
        window
    }

    fn full_twelve() -> Window {
        let mut window = Window::new(12);
        for i in 0..12 {
            window.store_mut().set(i, herb());
        }
        window
    }

    #[test]
    fn the_room_promised_fits_in_one_window() {
        let window = nine_of_twelve();

        assert_eq!(window.store().count_empty(), 3);
        assert_eq!(promised_room(&window), 3);
        assert_eq!(promised_room(&full_twelve()), 0);
    }

    /// The answer used to be slot 5, whose partner slot does not exist.
    #[test]
    fn a_two_slot_item_is_never_placed_against_the_edge() {
        let mut window = nine_of_twelve();

        let slot = seek_slot(&mut window, true);

        assert_eq!(window.position(), 6);
        assert_eq!(slot, 3);
        assert!((slot as usize) + 1 < BAG_SIZE);
    }

    #[test]
    fn a_single_item_takes_the_first_empty_slot_in_view() {
        let mut window = nine_of_twelve();

        assert_eq!(seek_slot(&mut window, false), 5);
        assert_eq!(window.position(), 4);
    }

    #[test]
    fn a_single_item_with_nothing_free_in_view_brings_the_room_into_view() {
        let mut window = Window::new(12);
        for i in 0..BAG_SIZE {
            window.store_mut().set(i, herb());
        }

        let slot = seek_slot(&mut window, false);

        assert!(slot >= 0 && (slot as usize) < BAG_SIZE);
        assert_eq!(window.store_index(slot as usize), Some(BAG_SIZE));
    }

    #[test]
    fn a_full_store_has_no_slot_for_anything() {
        let mut window = full_twelve();

        assert_eq!(seek_slot(&mut window, true), NO_EMPTY_SLOT);
        assert_eq!(seek_slot(&mut window, false), NO_EMPTY_SLOT);
    }
}
