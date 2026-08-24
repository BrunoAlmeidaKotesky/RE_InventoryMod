//! Replacements for the accessors that hand out a character's bag.
//!
//! These are the hinge of the whole design. The game asks for a bag every time
//! it needs one — including once per frame while drawing the inventory panel —
//! and uses whatever comes back. Answering with a bag this mod owns is what
//! makes a larger inventory visible; nothing else does, because changing memory
//! the accessor did not point at changes nothing the game reads.
//!
//! Each original does the same three things: read a global holding the current
//! characters, ask it which character this is, and return that character's bag
//! at `+0x20` or `+0x60` of the object it was called on. The two differ only in
//! which character they ask about.

use crate::core::logging::{log_debug, log_warn};
use crate::game::addresses::Addresses;
use crate::game::call::thiscall0;
use crate::store::registry;

/// Character ids whose bag lives at `+0x20`.
const FIRST_BAG_IDS: [i32; 3] = [1, 2, 3];
/// Character ids whose bag lives at `+0x60`.
const SECOND_BAG_IDS: [i32; 2] = [5, 7];

const FIRST_BAG_OFFSET: usize = 0x20;
const SECOND_BAG_OFFSET: usize = 0x60;

/// What the originals return for an id they do not recognise.
const NO_BAG: usize = 0;

/// Addresses the replacements need, filled in at install time.
///
/// A copy rather than a lookup: these run on the drawing path, and reaching
/// through a lock every frame to fetch a constant would be a poor trade.
static mut RESOLVED: Option<Addresses> = None;

/// # Safety
/// Call once, before either stub can run.
pub unsafe fn set_addresses(addresses: &Addresses) {
    RESOLVED = Some(*addresses);
}

fn addresses() -> Option<Addresses> {
    // Safety: written once at install time, before any hook is reachable.
    unsafe { RESOLVED }
}

/// Entry stub for the accessor that returns the played character's bag.
///
/// # Safety
/// Reached only through the detour over the original, so `ecx` holds the object
/// owning the bags and the caller expects the result in `eax`.
#[unsafe(naked)]
pub unsafe extern "C" fn player_bag_stub() {
    core::arch::naked_asm!(
        "push ecx",
        "call {handler}",
        "add esp, 4",
        "ret",
        handler = sym player_bag,
    )
}

/// Entry stub for the accessor that returns the partner's bag.
///
/// # Safety
/// Same contract as `player_bag_stub`.
#[unsafe(naked)]
pub unsafe extern "C" fn partner_bag_stub() {
    core::arch::naked_asm!(
        "push ecx",
        "call {handler}",
        "add esp, 4",
        "ret",
        handler = sym partner_bag,
    )
}

extern "C" fn player_bag(owner: usize) -> usize {
    let Some(addresses) = addresses() else {
        return NO_BAG;
    };
    resolve(owner, addresses.played_character)
}

extern "C" fn partner_bag(owner: usize) -> usize {
    let Some(addresses) = addresses() else {
        return NO_BAG;
    };
    resolve(owner, addresses.partner_character)
}

/// Works out which bag was asked for, and answers with this mod's view of it.
///
/// `character_getter` is the game function that names the character in
/// question; everything else is identical between the two accessors.
fn resolve(owner: usize, character_getter: usize) -> usize {
    let result = std::panic::catch_unwind(|| {
        let Some(addresses) = addresses() else {
            return NO_BAG;
        };

        if owner == 0 || !owner.is_multiple_of(4) {
            return NO_BAG;
        }

        let Some(offset) = bag_offset(owner, character_getter, &addresses) else {
            // Unknown character. The original answers null here, and so must
            // this: the caller checks for it.
            return NO_BAG;
        };

        // Safety: the game was about to return this address itself.
        let view = unsafe { registry::view_for(owner, offset) };

        if view.is_null() {
            // No store available. Fall back on the game's own bag, which leaves
            // that call unmodded rather than broken.
            log_debug!("No store for 0x{owner:08X}+0x{offset:02X}; using the game's bag.");
            return owner + offset;
        }

        view as usize
    });

    // The lock the store needs is released by now, and this runs on the game's
    // own thread mid-frame, which is the only safe place to ask for a redraw.
    unsafe { crate::hook::panel::redraw_if_requested() };

    // A panic must not unwind into the game. Null is what the original returns
    // for an unknown character, and every caller already handles it.
    result.unwrap_or_else(|_| {
        log_warn!("Bag accessor panicked.");
        NO_BAG
    })
}

/// Asks the game which character this is, and maps that to a bag offset.
fn bag_offset(owner: usize, character_getter: usize, addresses: &Addresses) -> Option<usize> {
    // Safety: the global is written by the game and read the same way here as
    // in the code being replaced.
    let holder = unsafe { *(addresses.character_holder as *const usize) };
    if holder == 0 {
        return None;
    }

    let character = unsafe { thiscall0(character_getter, holder) };
    if character == 0 {
        return None;
    }

    let id = unsafe { thiscall0(addresses.character_id, character) } as i32;

    if FIRST_BAG_IDS.contains(&id) {
        return Some(FIRST_BAG_OFFSET);
    }
    if SECOND_BAG_IDS.contains(&id) {
        return Some(SECOND_BAG_OFFSET);
    }

    log_debug!("Unrecognised character id {id} for owner 0x{owner:08X}.");
    None
}
