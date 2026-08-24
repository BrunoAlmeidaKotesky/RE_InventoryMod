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
//! These methods answer questions about a six-slot bag, and the answers to two
//! of them are used as write targets. So a replacement may never return an
//! index the game cannot address, however many slots the store behind it has.
//! Where the store has room but the window does not show it, the window moves
//! first and the answer names a slot that is now visible.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::inventory::{Bag, BAG_SIZE};
use crate::store::registry;

/// Calls logged per replacement before it goes quiet.
///
/// These run from menu code and can fire every frame. Enough lines to prove the
/// hook is live and returning sane values, then silence.
const LOG_LIMIT: usize = 8;

/// What the game's own search returns when there is no empty slot.
const NO_EMPTY_SLOT: i32 = -1;

static COUNT_EMPTY_CALLS: AtomicUsize = AtomicUsize::new(0);
static FIRST_EMPTY_CALLS: AtomicUsize = AtomicUsize::new(0);

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

/// How many empty slots the character has.
///
/// Counted across the whole store, not just the six the game can see. This is
/// what makes a larger inventory real: the caller uses it to decide whether an
/// item fits at all.
extern "C" fn count_empty(bag: *mut Bag) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        if !is_usable(bag) {
            log_warn!("count_empty called with an unusable pointer: {:?}", bag);
            return 0;
        }

        let empty = registry::with_view(bag, |window| window.store().count_empty() as i32)
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
/// Same contract as `count_empty_stub`.
#[unsafe(naked)]
pub unsafe extern "C" fn first_empty_stub() {
    core::arch::naked_asm!(
        "push ecx",
        "call {handler}",
        "add esp, 4",
        "ret",
        handler = sym first_empty,
    )
}

/// A slot the caller may write an item into, or `-1` if there is none.
///
/// The returned index is used directly as `bag->items[index]`, so it has to be
/// one of the six the game has. When the store has room outside the window, the
/// window moves onto it first and the index names its new visible position.
///
/// Two-slot items are not yet placed correctly here: they need two adjacent
/// free slots starting on an even index, and this answers with the first free
/// slot of any kind. The store's own repair pass fixes the alignment
/// afterwards, but the placement should choose better to begin with.
extern "C" fn first_empty(bag: *mut Bag) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        if !is_usable(bag) {
            log_warn!("first_empty called with an unusable pointer: {:?}", bag);
            return NO_EMPTY_SLOT;
        }

        let found = registry::with_view(bag, |window| {
            if let Some(slot) = window.first_visible_empty() {
                return slot as i32;
            }

            // Nothing free in view. If the store has room elsewhere, bring it
            // into view so the caller has somewhere legal to write.
            let Some(index) = window.store().first_empty() else {
                return NO_EMPTY_SLOT;
            };

            window.reveal(index);
            window
                .visible_slot(index)
                .map_or(NO_EMPTY_SLOT, |slot| slot as i32)
        })
        .unwrap_or_else(|| visible_first_empty(&*bag));

        if should_log(&FIRST_EMPTY_CALLS, "first_empty") {
            log_info!(
                "first_empty(0x{:08X}) = {}  [{}]",
                bag as usize,
                found,
                describe(&*bag)
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
