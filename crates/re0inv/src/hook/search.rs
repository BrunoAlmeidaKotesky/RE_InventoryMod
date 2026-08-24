//! Answering "is this item in the bag" for slots the game cannot see.
//!
//! # The bug this exists for
//!
//! Standing at a typewriter with an ink ribbon in slot 9 of twelve, the game
//! said there was no ribbon. Scrolling the panel until the ribbon was one of the
//! six visible slots made the option to save appear.
//!
//! That is the cost of the whole design, arriving on schedule. The game is
//! handed a six-slot bag and walks it; anything the window is not showing does
//! not exist as far as it is concerned. It applies to keys, to ammunition, to
//! every question the game asks about what the player is carrying.
//!
//! # Why one hook is enough
//!
//! Every such question funnels through `Bag::find_item(id)`. It dispatches to
//! two narrower searches, and both of those have exactly one caller: this
//! function. So this is the single place where an item can be missed.
//!
//! # How it answers
//!
//! By moving the window and asking the game again, one position at a time,
//! until the game itself says yes. What counts as a match covers item types,
//! the personal slot and a handful of special ids, and none of that is
//! reimplemented here — the game remains the judge of its own question.
//!
//! The window is then left where the item is visible. That is not a side
//! effect to be tidied up: the answer is a slot number, and the caller is about
//! to use it to reach into the six slots it can see. Putting the window back
//! would invalidate the number just handed over.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info};
use crate::game::inventory::Bag;
use crate::store::registry;

/// What the game returns when the item is not in the bag.
const NOT_FOUND: i32 = -1;

/// Searches logged before going quiet.
const LOG_LIMIT: usize = 12;

/// Where the trampoline rejoins the original, filled in at install time.
static CONTINUE: AtomicUsize = AtomicUsize::new(0);

static SWEEPS: AtomicUsize = AtomicUsize::new(0);

pub fn set_continue(address: usize) {
    CONTINUE.store(address, Ordering::Relaxed);
}

/// Entry stub for `Bag::find_item`.
///
/// # Safety
/// Reached only through the detour over the original, so `ecx` holds the bag
/// and the item id is the single stack argument, which the callee removes.
#[unsafe(naked)]
pub unsafe extern "C" fn find_item_stub() {
    core::arch::naked_asm!(
        // Arguments in reverse. Nothing has been pushed yet, so the id is still
        // where the caller left it.
        "push dword ptr [esp + 4]",
        "push ecx",
        "call {handler}",
        "add esp, 8",
        // __thiscall with one stack argument: the callee cleans it.
        "ret 4",
        handler = sym find_item,
    )
}

/// Calls the original, which the detour has jumped over the front of.
///
/// The first three instructions of the original are re-executed here and then
/// control joins it at the fourth, which is what the detour displaced. The
/// original ends in `ret 4`, so it removes the id pushed below.
///
/// # Safety
/// `CONTINUE` must hold the address of the fourth instruction, and the code
/// there must still be the original's.
#[unsafe(naked)]
unsafe extern "C" fn original(_bag: usize, _item_id: i32) -> i32 {
    core::arch::naked_asm!(
        "push ebp",
        "mov ebp, esp",
        // The object pointer belongs in ecx, and the id on the stack under the
        // return address the call below pushes.
        "mov ecx, [ebp + 8]",
        "push dword ptr [ebp + 12]",
        "call 2f",
        "mov esp, ebp",
        "pop ebp",
        "ret",
        // The three instructions the detour overwrote.
        "2:",
        "sub esp, 8",
        "push esi",
        "push edi",
        "jmp dword ptr [{continue_at}]",
        continue_at = sym CONTINUE,
    )
}

extern "C" fn find_item(bag: usize, item_id: i32) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if bag == 0 || !bag.is_multiple_of(4) {
            return NOT_FOUND;
        }

        let ask = || unsafe { original(bag, item_id) };

        // The overwhelming majority of searches are for something in view, and
        // those cost one call and nothing else.
        let found = ask();
        if found != NOT_FOUND {
            return found;
        }

        sweep(bag as *mut Bag, item_id, ask)
    });

    result.unwrap_or(NOT_FOUND)
}

/// Looks through the rest of the store by showing it to the game.
fn sweep(bag: *mut Bag, item_id: i32, mut ask: impl FnMut() -> i32) -> i32 {
    // The box is storage of the same kind and is searched the same way, but it
    // lives outside the registry, so it has to be offered the sweep too.
    let found = registry::probe_positions(bag, &mut ask)
        .or_else(|| crate::feature::item_box::probe_positions(bag, &mut ask));

    let Some(found) = found else {
        // Either the bag is not one of ours, in which case the first answer was
        // the whole truth, or the item really is not there.
        return NOT_FOUND;
    };

    let seen = SWEEPS.fetch_add(1, Ordering::Relaxed);
    if seen < LOG_LIMIT {
        log_info!("Item {item_id} was out of view; the panel moved to slot {found}.");
    } else if seen == LOG_LIMIT {
        log_debug!("Out-of-view searches: logged {LOG_LIMIT}, staying quiet from here.");
    }

    // The window moved, so anything already on screen is now showing the wrong
    // slots. Harmless outside the menu, where nothing is drawn.
    crate::hook::panel::request_redraw();

    found
}
