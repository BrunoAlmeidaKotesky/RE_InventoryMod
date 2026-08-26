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
    // A screen that is not up has no panel to rebuild, and its object may
    // already be freed — which is what a redraw against a stale pointer would
    // find. Checked before the request is taken so it stays pending.
    if !is_open() {
        return;
    }

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

/// The partner half's own selection, `menu+0x2BC`.
///
/// Proved by the navigation code at `0x005E3BD1`-`0x005E3D6F`, which reads and
/// writes exactly this field while the selection is over there. The played
/// half's cursor above freezes at its last value for that whole time.
const OFFSET_PARTNER_CURSOR: usize = 0x2BC;

/// The menu phase while the selection is in the partner's half.
pub const PHASE_PARTNER_HALF: i32 = 7;

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

/// Where the selection is inside the partner's half, while it is over there.
pub fn partner_cursor() -> Option<i32> {
    let menu = menu()?;
    crate::debug::memory::read_i32(menu + OFFSET_PARTNER_CURSOR)
}

// --- Making the partner half usable as the box ---

/// Set while the partner's half of the screen is on show.
///
/// Without it the game draws the player's inventory alone, which is what it
/// does whenever the two characters are apart — and standing at a typewriter,
/// they usually are.
const OFFSET_PARTNER_SHOWN: usize = 0x2CA;

/// Whether items may be moved between the two halves. Zero allows it.
///
/// The game sets this from how close the partner is. The box is not a partner
/// and is never far away, so while it is showing this is forced open and the
/// old value put back afterwards.
const OFFSET_EXCHANGE: usize = 0x28B;

const EXCHANGE_ALLOWED: u8 = 0;

/// What the game had in the exchange field before the box took it over.
static SAVED_EXCHANGE: Mutex<Option<u8>> = Mutex::new(None);

/// Says once what the fields held before the box changed them.
static FORCED: AtomicBool = AtomicBool::new(false);

/// Marks the partner half as being on screen.
///
/// # Safety
/// `menu` must be a live menu object.
pub unsafe fn mark_partner_shown(menu: usize) {
    let shown = (menu + OFFSET_PARTNER_SHOWN) as *mut u8;
    if shown.read_volatile() == 0 {
        shown.write_volatile(1);
    }
}

/// Forces exchanging open, remembering what the game had there.
///
/// # Safety
/// `menu` must be a live menu object.
pub unsafe fn allow_exchange(menu: usize) {
    let exchange = (menu + OFFSET_EXCHANGE) as *mut u8;
    let current = exchange.read_volatile();

    if current == EXCHANGE_ALLOWED {
        return;
    }

    if let Ok(mut saved) = SAVED_EXCHANGE.lock() {
        // Only the first value seen is the game own. Anything later is
        // whatever it recomputed while the box was already showing.
        saved.get_or_insert(current);
    }

    exchange.write_volatile(EXCHANGE_ALLOWED);
}

/// Puts the exchange field back, for a menu we still have in hand.
///
/// # Safety
/// `menu` must be a live menu object.
#[cfg(feature = "itembox")]
pub unsafe fn restore_exchange(menu: usize) {
    let Ok(saved) = SAVED_EXCHANGE.lock() else {
        return;
    };

    if let Some(original) = *saved {
        ((menu + OFFSET_EXCHANGE) as *mut u8).write_volatile(original);
    }
}

/// Puts the menu into the state the box needs, remembering what it replaced.
///
/// Called on every draw as well as from the screen own setup. The game writes
/// both fields itself, so a single write at the start would be undone by its
/// own bookkeeping.
///
/// # Safety
/// `menu` must be the object the panel was drawn against, which is alive for as
/// long as the screen is.
unsafe fn show_partner_half(menu: usize) {
    if !FORCED.swap(true, Ordering::Relaxed) {
        log_info!(
            "Box showing: partner half {}, exchange {}.",
            ((menu + OFFSET_PARTNER_SHOWN) as *const u8).read_volatile(),
            ((menu + OFFSET_EXCHANGE) as *const u8).read_volatile()
        );
    }

    mark_partner_shown(menu);
    allow_exchange(menu);
}

/// Puts the exchange field back to whatever the game had in it.
///
/// # Safety
/// The menu object must still be alive, which is why this runs while the screen
/// is closing rather than after.
#[cfg(feature = "itembox")]
pub unsafe fn restore_partner_half() {
    let Ok(mut saved) = SAVED_EXCHANGE.lock() else {
        return;
    };

    FORCED.store(false, Ordering::Relaxed);

    let Some(original) = saved.take() else {
        return;
    };

    let Some(menu) = menu() else { return };

    ((menu + OFFSET_EXCHANGE) as *mut u8).write_volatile(original);
    log_debug!("Exchange state put back to {original}.");
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

            // The box lives in the partner's half, and that half is only drawn
            // and only interactive when the game thinks a partner is there.
            if crate::feature::item_box::is_open() {
                // Safety: this is the object the game is drawing against right
                // now, so it is alive and these are its own fields.
                unsafe { show_partner_half(menu) };
            }
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
