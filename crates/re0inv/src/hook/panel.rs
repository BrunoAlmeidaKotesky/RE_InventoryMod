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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

/// Entry point of the drawing function, so it can be called deliberately.
static DRAW: AtomicUsize = AtomicUsize::new(0);

/// When the panel was last reported out of date.
static REQUESTED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Guards against redrawing from inside a redraw. The drawing function asks for
/// the bags, and answering that is where the redraw is triggered from.
static REDRAWING: AtomicBool = AtomicBool::new(false);

pub fn set_continue(address: usize) {
    CONTINUE.store(address, Ordering::Relaxed);
}

pub fn set_draw(address: usize) {
    DRAW.store(address, Ordering::Relaxed);
}

/// How long a redraw request stays worth honouring.
///
/// The request is made from the mod's own thread and honoured from the game's,
/// on the next accessor call. Those calls happen every frame whether or not the
/// inventory is open, so a request left standing would eventually be honoured
/// against a menu the player has already closed. Scrolling and the next frame
/// are milliseconds apart; anything older than this did not get its chance and
/// is not going to.
const REDRAW_DEADLINE: Duration = Duration::from_millis(500);

/// Says the panel is out of date.
///
/// The item descriptions come off the bag every frame and update on their own,
/// but the icons are built once when the inventory opens. So changing what the
/// bag holds is not enough on its own: the drawing has to be asked to run
/// again.
pub fn request_redraw() {
    if let Ok(mut requested) = REQUESTED_AT.lock() {
        *requested = Some(Instant::now());
    }
}

/// Takes the pending request, if there is one and it is still fresh.
fn take_request() -> bool {
    let Ok(mut requested) = REQUESTED_AT.lock() else {
        return false;
    };

    match requested.take() {
        Some(at) if at.elapsed() < REDRAW_DEADLINE => true,
        Some(at) => {
            log_info!("Redraw request expired after {} ms.", at.elapsed().as_millis());
            false
        }
        None => false,
    }
}

/// Redraws the panel if it was asked for, and if that is safe here.
///
/// Called from the bag accessor, which the game reaches on its own thread while
/// a frame is in progress. Doing it from the mod's input thread instead would
/// mean drawing while the game is halfway through drawing.
///
/// # Safety
/// The menu object must still be alive, and the caller must hold no lock the
/// drawing path will want: it asks for the bags, which comes straight back
/// through this mod.
pub unsafe fn redraw_if_requested() {
    if !take_request() {
        return;
    }

    let Some(menu) = menu() else {
        log_info!("Redraw wanted, but the panel has never been drawn.");
        return;
    };

    let draw = DRAW.load(Ordering::Relaxed);
    if draw == 0 {
        log_info!("Redraw wanted, but the drawing address is unknown.");
        return;
    }

    // The drawing function asks for a bag, which lands back here. Without this
    // the first redraw would ask for another, forever.
    if REDRAWING.swap(true, Ordering::Relaxed) {
        log_info!("Redraw wanted, but one is already running.");
        return;
    }

    log_info!("Redrawing panel 0x{menu:08X} through 0x{draw:08X}.");

    // Cleared by the guard even if the call below unwinds, which a plain store
    // after it would not do. A stuck flag would silently disable every redraw
    // from then on.
    let _guard = RedrawGuard;

    crate::game::call::thiscall0(draw, menu);
}

/// Cursor position within the panel, 0 to 5. Two columns, so a row is two.
///
/// Found by differential scan, not by guesswork: `0x2BC` was tried first and
/// read zero for an entire session of navigating the panel. This one was the
/// first field in the object to move repeatedly between values in the range a
/// six-slot selection can hold, and it moved from the first menu open onwards.
///
/// The neighbours at `0x2AC`, `0x2B0` and `0x2BC` behave similarly and are
/// logged alongside it under the probe switch, so a wrong pick here shows up in
/// one run rather than in another round of guessing.
const OFFSET_CURSOR: usize = 0x2B4;

/// The fields that looked like a selection, for telling them apart.
pub const CURSOR_CANDIDATES: [usize; 4] = [0x2AC, 0x2B0, 0x2B4, 0x2BC];
/// Counter the menu bumps as it moves between states.
const OFFSET_PHASE: usize = 0x294;

/// Slots across the panel. A vertical move is a step of this.
pub const COLUMNS: i32 = 2;

/// One past the highest cursor value the game uses.
const CURSOR_LIMIT: i32 = 6;

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

    // Reading first proves the page is still mapped, and the range proves it
    // still looks like a cursor rather than whatever was allocated over a menu
    // that has been closed.
    let plausible = crate::debug::memory::read_i32(menu + OFFSET_CURSOR)
        .is_some_and(|current| (0..CURSOR_LIMIT).contains(&current));

    if !plausible {
        return false;
    }

    // Volatile: the game reads this field on its own thread, so the write must
    // not be reordered or folded away.
    ((menu + OFFSET_CURSOR) as *mut i32).write_volatile(value);
    true
}

/// The menu's state counter, for telling an open inventory from a closed one.
pub fn phase() -> Option<i32> {
    let menu = menu()?;
    crate::debug::memory::read_i32(menu + OFFSET_PHASE)
}

/// All four selection candidates at once, for identifying which is which.
pub fn cursor_candidates() -> Option<[Option<i32>; 4]> {
    let menu = menu()?;
    Some(CURSOR_CANDIDATES.map(|at| crate::debug::memory::read_i32(menu + at)))
}

/// The phase the menu rests at while the inventory is not on screen.
const PHASE_CLOSED: i32 = 0;

/// Whether the inventory screen is up.
///
/// Every open and close logged so far runs `0 -> 1 -> 2`, stays at two or above
/// for as long as the screen is up, then `-> 3 -> 0`. So anything but zero means
/// open, and that is a far better answer than the one this replaced: the
/// selection had not moved for a while, which at a typewriter is always true.
pub fn is_open() -> bool {
    phase().is_some_and(|phase| phase != PHASE_CLOSED)
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

/// Clears the re-entry flag however the redraw ends.
struct RedrawGuard;

impl Drop for RedrawGuard {
    fn drop(&mut self) {
        REDRAWING.store(false, Ordering::Relaxed);
    }
}
