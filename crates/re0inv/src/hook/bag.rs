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

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::inventory::{Bag, BAG_SIZE};

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
///
/// Returns whether this call was logged, so the caller can build the message
/// only when it will be used.
fn should_log(calls: &AtomicUsize, name: &str) -> bool {
    let seen = calls.fetch_add(1, Ordering::Relaxed);

    if seen == LOG_LIMIT {
        log_debug!("{name}: logged {LOG_LIMIT} calls, staying quiet from here.");
    }

    seen < LOG_LIMIT
}

/// Reads the bag the game passed, if it looks like a bag at all.
///
/// # Safety
/// `bag` is whatever the game had in `ecx`. It is trusted only as far as being
/// non-null and readable, which is all that can be checked from here.
unsafe fn bag_ref<'a>(bag: *const Bag) -> Option<&'a Bag> {
    if bag.is_null() {
        return None;
    }

    // A misaligned pointer would mean we are not being called as a method at
    // all, which is worth knowing before dereferencing it.
    if !(bag as usize).is_multiple_of(4) {
        return None;
    }

    Some(&*bag)
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

/// How many of the bag's slots are empty.
///
/// This is a faithful reimplementation of what the game's own function does:
/// walk the six slots, count the ones with item id zero. It exists to prove the
/// detour and the calling convention are right before anything starts returning
/// different answers than the game expects.
extern "C" fn count_empty(bag: *const Bag) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        let Some(bag) = bag_ref(bag) else {
            log_warn!("count_empty called with an unusable pointer: {:?}", bag);
            return 0;
        };

        let empty = bag.items.iter().filter(|item| item.is_empty()).count() as i32;

        if should_log(&COUNT_EMPTY_CALLS, "count_empty") {
            log_info!(
                "count_empty(0x{:08X}) = {} of {}  [{}]",
                bag as *const Bag as usize,
                empty,
                BAG_SIZE,
                describe(bag)
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

/// Index of the bag's first empty slot, or `-1` when it is full.
///
/// Another faithful reimplementation. This one is reached whenever the game
/// puts any item into a bag, not only a two-slot one, which makes it the cheap
/// way to prove the hooks are live.
extern "C" fn first_empty(bag: *const Bag) -> i32 {
    let result = std::panic::catch_unwind(|| unsafe {
        let Some(bag) = bag_ref(bag) else {
            log_warn!("first_empty called with an unusable pointer: {:?}", bag);
            return NO_EMPTY_SLOT;
        };

        let found = bag
            .items
            .iter()
            .position(|item| item.is_empty())
            .map_or(NO_EMPTY_SLOT, |index| index as i32);

        if should_log(&FIRST_EMPTY_CALLS, "first_empty") {
            log_info!(
                "first_empty(0x{:08X}) = {}  [{}]",
                bag as *const Bag as usize,
                found,
                describe(bag)
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

/// The bag's item ids, for the log.
fn describe(bag: &Bag) -> String {
    bag.items
        .iter()
        .map(|item| item.id.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
