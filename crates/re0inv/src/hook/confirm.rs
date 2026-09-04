//! Confirming an item while the selection rests on the tail of a two-slot one.
//!
//! A two-slot item is its head followed by a filler, id 180. The game's own
//! moves mostly keep the selection off the filler: moving up onto the right
//! column checks the left neighbour and pulls the selection onto the head
//! (`0x005E2019`-`0x005E206C`), and moving right steps over the whole item.
//! Moving *down* the right column does not check (`0x005E20D6`-`0x005E2153`),
//! so a two-slot item in the bottom row can be reached on its tail.
//!
//! Confirming there copies the tail's slot into `+0x2B8` (`0x005E382B`), and
//! Examine builds its viewer for item 180. That item has no data, the viewer's
//! keyframe lookup finds no range for it (`0x005F5A90`), and the assertion at
//! `0x005F5ABE` ends the game. With six slots the layout that allows it is
//! rare; with a window over twelve it is ordinary.
//!
//! The store this instruction is replaced with saves the head instead, and
//! pulls the selection onto it the way the game's own up-move does, so every
//! action that follows sees the item and not its filler.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::log_info;
use crate::game::inventory::SLOT_TWO_FILLER;

/// Where the trampoline rejoins the game, filled in at install time.
static CONTINUE: AtomicUsize = AtomicUsize::new(0);

pub fn set_continue(address: usize) {
    CONTINUE.store(address, Ordering::Relaxed);
}

/// `mov [edi+0x2B8], eax`: the cursor being saved for the action submenu.
pub const SAVE_SLOT: [u8; 6] = [0x89, 0x87, 0xB8, 0x02, 0x00, 0x00];

/// Trampoline over the save of the confirmed slot.
///
/// # Safety
/// Reached only through the jump written over that instruction, so `edi` is
/// the menu object and `eax` the cursor it was about to save. The handler does
/// the save itself; nothing after the instruction reads `eax`.
#[unsafe(naked)]
pub unsafe extern "C" fn confirm_stub() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], edi",
        "mov [esp + 4], eax",
        "call {handler}",
        "mov esp, ebp",
        "popad",
        "popfd",
        "jmp dword ptr [{continue_at}]",
        handler = sym confirm,
        continue_at = sym CONTINUE,
    )
}

extern "C" fn confirm(menu: usize, cursor: i32) {
    let _ = std::panic::catch_unwind(|| {
        let head = head_of(cursor);

        // Safety: the game was about to write this field of this object.
        unsafe { crate::hook::panel::save_confirmed_slot(menu, head, head != cursor) };

        if head != cursor {
            log_info!("Confirmed on the tail of a two-slot item; saved slot {head} instead of {cursor}.");
        }
    });
}

/// The slot to save: `cursor`, unless the item there is a filler, in which
/// case the head just before it.
fn head_of(cursor: i32) -> i32 {
    let Ok(slot) = usize::try_from(cursor) else {
        return cursor;
    };

    if slot == 0 {
        return cursor;
    }

    let Some(offset) = crate::hook::accessor::played_offset() else {
        return cursor;
    };

    let on_tail = crate::store::registry::item_in_view(offset, slot)
        .is_some_and(|item| item.id == SLOT_TWO_FILLER);

    if on_tail {
        cursor - 1
    } else {
        cursor
    }
}
