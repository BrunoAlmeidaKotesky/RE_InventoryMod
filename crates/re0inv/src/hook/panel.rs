//! Watching the inventory panel being drawn.
//!
//! The store scrolls and the accessors hand the game the scrolled view, but the
//! panel only shows it after the inventory is closed and opened again. So the
//! panel is built when the menu opens rather than redrawn each frame, and the
//! remaining problem is making it rebuild on demand.
//!
//! This records when the drawing runs and, more usefully, the menu object it
//! runs against. That object is the missing argument: with it, a redraw can be
//! asked for rather than waited on.
//!
//! Nothing here changes behaviour. The original instructions are re-executed
//! and control goes straight back.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info};

/// Draws logged before going quiet. Enough to tell "once per open" from "once
/// per frame" without filling the file.
const LOG_LIMIT: usize = 16;

static DRAWS: AtomicUsize = AtomicUsize::new(0);

/// The menu object the panel was last drawn against.
///
/// Kept so a redraw can be requested later. Zero until the inventory has been
/// opened at least once.
static MENU: AtomicUsize = AtomicUsize::new(0);

/// Where the trampoline hands control back, filled in at install time.
static CONTINUE: AtomicUsize = AtomicUsize::new(0);

pub fn set_continue(address: usize) {
    CONTINUE.store(address, Ordering::Relaxed);
}

/// Cursor position within the panel, 0 to 5. Two columns, so a row is two.
const OFFSET_CURSOR: usize = 0x2BC;
/// Counter the menu bumps as it moves between states.
const OFFSET_PHASE: usize = 0x294;

/// Slots across the panel. A vertical move is a step of this.
pub const COLUMNS: i32 = 2;

/// The menu object, if the inventory has been drawn at least once.
pub fn menu() -> Option<usize> {
    match MENU.load(Ordering::Relaxed) {
        0 => None,
        address => Some(address),
    }
}

/// Where the selection is, as the game sees it.
///
/// Read straight out of the menu object rather than by intercepting the code
/// that moves it. Several attempts at finding that code hooked instructions the
/// game never reaches; the field itself is not in any doubt.
pub fn cursor() -> Option<i32> {
    let menu = menu()?;
    crate::debug::memory::read_i32(menu + OFFSET_CURSOR)
}

/// Moves the selection.
///
/// Writing this is what the game itself does when the cursor moves, so it is
/// also the most likely way to make the panel notice that something changed.
///
/// # Safety
/// The menu object must still be alive, which is true while the inventory is
/// open and stops being true at some point after it closes. The value is a
/// single aligned word, so a stale write lands in freed memory rather than
/// tearing a structure.
pub unsafe fn set_cursor(value: i32) -> bool {
    let Some(menu) = menu() else { return false };

    if crate::debug::memory::read_i32(menu + OFFSET_CURSOR).is_none() {
        return false;
    }

    *((menu + OFFSET_CURSOR) as *mut i32) = value;
    true
}

/// The menu's state counter, for telling an open inventory from a closed one.
pub fn phase() -> Option<i32> {
    let menu = menu()?;
    crate::debug::memory::read_i32(menu + OFFSET_PHASE)
}

/// How many times the panel has been drawn.
pub fn draw_count() -> usize {
    DRAWS.load(Ordering::Relaxed)
}

/// Trampoline over the start of the panel drawing function.
///
/// # Safety
/// Reached only through the detour written over the first three instructions of
/// that function, so `ecx` holds the menu object. Those instructions are
/// re-executed here before control returns.
#[unsafe(naked)]
pub unsafe extern "C" fn draw_stub() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], ecx",
        "call {observe}",
        "mov esp, ebp",
        "popad",
        "popfd",
        // The three instructions this replaced.
        "sub esp, 8",
        "push esi",
        "mov esi, ecx",
        "jmp dword ptr [{continue_at}]",
        observe = sym observe,
        continue_at = sym CONTINUE,
    )
}

extern "C" fn observe(menu: usize) {
    let _ = std::panic::catch_unwind(|| {
        if menu != 0 && menu.is_multiple_of(4) {
            MENU.store(menu, Ordering::Relaxed);
        }

        let seen = DRAWS.fetch_add(1, Ordering::Relaxed);

        if seen < LOG_LIMIT {
            log_info!("Panel drawn ({}) against menu 0x{menu:08X}.", seen + 1);
        } else if seen == LOG_LIMIT {
            log_debug!("Panel draws: logged {LOG_LIMIT}, staying quiet from here.");
        }
    });
}
