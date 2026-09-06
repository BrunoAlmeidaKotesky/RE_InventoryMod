//! Turning the player's own navigation into a scroll.
//!
//! The panel shows six slots and the store holds more. The wanted behaviour is
//! the obvious one: with the selection on the bottom row, pressing down brings
//! the next rows into view instead of doing nothing.
//!
//! Only downwards. Pressing up on the top row already moves to the tabs above
//! the panel, and taking that over would break something the player uses. The
//! list wraps instead, so everything is reachable by carrying on down.
//!
//! The game's own cursor code refuses the move at the edge, and several
//! attempts at intercepting that refusal hooked instructions it never reaches.
//! So the selection is read out of the menu object instead, and the direction is
//! read from the keyboard and the controller here. That works with either, and
//! needs nothing understood about how the menu dispatches input.
//!
//! What it does not do is follow the player's own key bindings. Rebinding the
//! menu keys would leave scrolling on the defaults, which is the reason to move
//! this into the game's input handling eventually.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::core::gamepad::{
    Controller, BUTTON_DPAD_DOWN, BUTTON_LEFT_THUMB, BUTTON_RIGHT_THUMB,
};
use crate::core::logging::{log_debug, log_info};
use crate::debug::probe;
use crate::game::inventory::{BAG_SIZE, SLOT_TWO_FILLER};
use crate::hook::panel;
use crate::store::registry;
use crate::win32::{GetAsyncKeyState, KEY_PRESSED};

const VK_DOWN: i32 = 0x28;
const VK_S: i32 = 0x53;

/// Shows the item box in place of the partner's bag.
const VK_HOME: i32 = 0x24;

const VK_PAGE_UP: i32 = 0x21;
const VK_PAGE_DOWN: i32 = 0x22;

const VK_F7: i32 = 0x76;
const VK_F8: i32 = 0x77;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;

const COMMAND_KEYS: [i32; 9] = [
    VK_HOME,
    VK_PAGE_UP,
    VK_PAGE_DOWN,
    VK_F7,
    VK_F8,
    VK_F9,
    VK_F10,
    VK_F11,
    VK_F12,
];

/// How far the stick must be pushed to count as a direction. Well past the
/// resting wobble, so a controller sitting on a desk scrolls nothing.
const STICK_THRESHOLD: i16 = 16_000;

const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// One row of two slots per press.
const SCROLL_STEP: i32 = 1;

pub fn run(ini: PathBuf, debug_keys: bool) {
    #[cfg(any(feature = "expanded", feature = "itembox"))]
    {
        log_info!("Inventory scrolling: press down on the bottom row, or click the right stick.");
        log_info!("Page Up and Page Down also scroll it directly.");
    }
    #[cfg(feature = "itembox")]
    log_info!("Item box: choose it on the typewriter prompt, or press Home in the inventory.");
    if debug_keys {
        log_info!("Debug keys: F7 selection scan, F8 remove hooks, F9 scan, F10 narrow, F11 inspect, F12 memory map.");
    }

    let controller = Controller::load();

    let mut command_was_down = [false; COMMAND_KEYS.len()];
    let mut down_was_down = false;
    let mut cycle_was_down = false;
    let mut box_was_down = false;
    let mut pad_seen = false;

    let mut last_cursor: Option<i32> = None;
    let mut last_phase: Option<i32> = None;
    let mut menu_was_open = false;
    // The bag offset whose row is being held, while one is.
    let mut held_row: Option<usize> = None;
    let mut last_fields: Option<panel::Snapshot> = None;
    let mut last_active: Option<(Target, i32)> = None;

    loop {
        crate::debug::hang::beat();
        for (index, &key) in COMMAND_KEYS.iter().enumerate() {
            let down = pressed(key);

            if down && !command_was_down[index] {
                dispatch_command(key, &ini, debug_keys);
            }
            command_was_down[index] = down;
        }

        let cursor = panel::cursor();
        if cursor != last_cursor {
            last_cursor = cursor;

            // All four candidates together, so a wrong pick is obvious from one
            // session rather than needing another round of guessing.
            if debug_keys {
                log_debug!(
                    "Selection {cursor:?}; candidates {:?} at {:X?}.",
                    panel::cursor_candidates(),
                    panel::CURSOR_CANDIDATES
                );
            }
        }

        if debug_keys {
            crate::debug::selection::sample();
        }

        let phase = panel::phase();
        if phase != last_phase {
            log_debug!(
                "Menu phase {:?} -> {:?}, cursor {:?}",
                last_phase,
                phase,
                cursor
            );
            last_phase = phase;
        }

        // The box belongs to one visit to the inventory screen. Watching the
        // screen close is what ends it.
        let menu_open = panel::is_open();
        if menu_was_open && !menu_open {
            crate::feature::item_box::close_with_menu();
            // The game is about to read the equipped index off the bags; a
            // weapon scrolled out of view would read as holstered.
            registry::keep_equipped_visible_all();
        }
        menu_was_open = menu_open;

        // The fields the mod reasons about, whenever one of them changes.
        // This is how the phases and modes above were pinned down, and how a
        // wrong reading shows up in one run instead of a round of guessing.
        let fields = if menu_open { panel::snapshot() } else { None };
        if fields != last_fields {
            if let Some(fields) = fields {
                log_debug!("Menu {fields:?}");
            }
            last_fields = fields;
        }

        // While a second item is being chosen, the first one's row is held
        // still, so the slot the game saved keeps naming it however far the
        // rest of the store scrolls. Released the moment the choice is over.
        let choosing_second = menu_open && panel::phase() == Some(panel::PHASE_SECOND_ITEM);
        if choosing_second && held_row.is_none() {
            held_row = hold_first_item();
        } else if !choosing_second {
            if let Some(offset) = held_row.take() {
                registry::unpin(offset, panel::cursor().and_then(|c| usize::try_from(c).ok()));
                log_info!("The held row is released.");
            }
        }
        ROW_HELD.store(held_row.is_some(), Ordering::Relaxed);

        // The game's own down-move lands on the tail of a two-slot item when
        // it comes down the right column (`0x005E20D6`, `0x005E4CC9`); its
        // up-move is the only one that pulls onto the head. So whichever
        // selection is live is checked whenever it moves, not only after a
        // scroll of ours, and pulled the way the up-move would have.
        let active = active_selection();
        if active != last_active {
            last_active = active;
            if let Some((target, _)) = active {
                settle_cursor(target);
            }
        }

        let down = holding_down(&controller, &mut pad_seen);

        if down && !down_was_down {
            try_edge_scroll(cursor);
        }

        down_was_down = down;

        let buttons = controller.as_ref().map_or(0, |pad| pad.buttons());

        // Clicking the right stick scrolls outright, wherever the selection is.
        // Reading the cursor to know when the player is pressing against the
        // bottom row is the nicer behaviour and it is kept, but it depends on
        // the cursor field and on our own reading of the pad. This does not.
        let cycle = buttons & BUTTON_RIGHT_THUMB != 0;
        if cycle && !cycle_was_down {
            scroll_or_wrap();
        }
        cycle_was_down = cycle;

        // Clicking the left stick shows the box, the same as Home does.
        let show_box = buttons & BUTTON_LEFT_THUMB != 0;
        if show_box && !box_was_down {
            toggle_box();
        }
        box_was_down = show_box;

        // Hands the box its contents from the side file once a bag restore has
        // vouched for it. Almost always a no-op.
        #[cfg(any(feature = "expanded", feature = "itembox"))]
        crate::save::settle();

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// What a scroll would move right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    /// The characters' bags, together.
    Bags,
    /// The item box, standing in for the partner's bag.
    Box,
    /// The partner's own bag alone, at this offset: the target of an
    /// exchange, while the played half holds the item being given.
    Partner(usize),
}

/// Where a scroll goes at this moment, if anywhere.
///
/// Only two phases move a live cursor over a window this mod can slide:
/// browsing the played half, and choosing the partner-side slot of an
/// exchange while the box is what the partner's half shows. Everywhere else
/// the game is holding an index it took earlier — the slot confirmed for the
/// action submenu, saved at `+0x2B8` — and sliding the window under it hands
/// Use, Combine and Examine a different item than the one on screen. That is
/// exactly what happened: pressing down to reach "Examine" scrolled the bag
/// underneath, and Examine showed whatever had slid under the saved slot.
///
/// The partner's real bag is left alone even in its own phase: the saved
/// slot belongs to the played half, and the bags only scroll together.
///
/// Read here and acted on a moment later, on a different thread from the one
/// that changes the phase. The game could confirm an item in between, which
/// would take the phase past two with the scroll still to come. The window
/// for that is one poll, forty milliseconds, and needs the player to press
/// confirm and down at the same instant; closing it means sliding the window
/// from the game's own thread instead, which is a larger change than the
/// exposure warrants today.
fn scroll_target() -> Option<Target> {
    match panel::phase()? {
        panel::PHASE_PARTNER_HALF if crate::feature::item_box::is_open() => Some(Target::Box),
        // An exchange aimed at the partner: only the partner's bag slides.
        // The played half holds the item being given by its slot number, so
        // it must stay exactly where it is.
        panel::PHASE_PARTNER_HALF => crate::hook::accessor::partner_offset().map(Target::Partner),
        panel::PHASE_BROWSING => Some(Target::Bags),
        // Choosing the second item of a Combine: allowed once the first
        // item's row is held still, which is what keeps `+0x2B4` true.
        panel::PHASE_SECOND_ITEM if ROW_HELD.load(Ordering::Relaxed) => Some(Target::Bags),
        _ => None,
    }
}

/// Whether a row of the played bag is currently held still.
static ROW_HELD: AtomicBool = AtomicBool::new(false);

/// The selection the player is moving right now, and where it is.
fn active_selection() -> Option<(Target, i32)> {
    let target = scroll_target()?;
    let cursor = match target {
        Target::Bags => played_cursor()?,
        Target::Box | Target::Partner(_) => panel::partner_cursor()?,
    };
    Some((target, cursor))
}

/// The cursor that moves over the played half right now: the second one
/// while a second item is being chosen, the first otherwise.
fn played_cursor() -> Option<i32> {
    if panel::phase() == Some(panel::PHASE_SECOND_ITEM) {
        panel::second_cursor()
    } else {
        panel::cursor()
    }
}

/// Holds the first item's row; returns the bag offset it was held in.
fn hold_first_item() -> Option<usize> {
    let offset = crate::hook::accessor::played_offset()?;
    let first = usize::try_from(panel::cursor()?).ok()?;

    if !registry::pin_row(offset, first / 2) {
        return None;
    }

    log_info!(
        "Choosing a second item: row {} stays put while the rest of the bag scrolls.",
        first / 2 + 1
    );
    Some(offset)
}

/// What every scroll ends with: the selection checked against what slid
/// under it, the panel redrawn, the log told.
fn after_scroll(target: Target) {
    settle_cursor(target);
    panel::request_redraw();
    report();
}

/// Pulls the selection off the tail of a two-slot item the scroll left it on.
///
/// The game never lets the selection rest on a tail; a window sliding under
/// a still selection can. Examining the tail asks the item table about the
/// filler, which ends the game, so the selection is pulled onto the head the
/// way the game's own moves do it.
fn settle_cursor(target: Target) {
    let slot = |cursor: Option<i32>| cursor.and_then(|c| usize::try_from(c).ok());

    let second = panel::phase() == Some(panel::PHASE_SECOND_ITEM);

    let on_tail = match target {
        Target::Bags => {
            let Some(offset) = crate::hook::accessor::played_offset() else { return };
            let Some(cursor) = slot(played_cursor()) else { return };
            registry::item_in_view(offset, cursor).is_some_and(|item| item.id == SLOT_TWO_FILLER)
        }
        Target::Box => {
            let Some(cursor) = slot(panel::partner_cursor()) else { return };
            crate::feature::item_box::item_in_view(cursor)
                .is_some_and(|item| item.id == SLOT_TWO_FILLER)
        }
        Target::Partner(offset) => {
            let Some(cursor) = slot(panel::partner_cursor()) else { return };
            registry::item_in_view(offset, cursor).is_some_and(|item| item.id == SLOT_TWO_FILLER)
        }
    };

    if !on_tail {
        return;
    }

    log_info!("The selection was left on the tail of a two-slot item; pulled onto its head.");

    // Safety: the screen is up, or the scroll would not have been allowed.
    unsafe {
        match target {
            Target::Bags if second => panel::pull_second_cursor_left(),
            Target::Bags => panel::pull_cursor_left(),
            Target::Box | Target::Partner(_) => panel::pull_partner_cursor_left(),
        }
    }
}

fn scroll_by(target: Target, rows: i32) -> bool {
    match target {
        Target::Bags => registry::scroll_all(rows) > 0,
        Target::Box => crate::feature::item_box::scroll(rows),
        Target::Partner(offset) => registry::scroll_offset(offset, rows),
    }
}

/// Back to the first slot, for carrying on past the end.
fn rewind(target: Target) -> bool {
    match target {
        Target::Bags => registry::rewind_all() > 0,
        Target::Box => crate::feature::item_box::scroll(i32::MIN / 2),
        Target::Partner(offset) => registry::scroll_offset(offset, i32::MIN / 2),
    }
}

/// Scrolls when the selection is pressed against the bottom row.
///
/// Which half the selection is in decides which cursor to read: in the
/// partner's half the game keeps a separate one, and the played half's
/// freezes at its last value; reading the frozen one here is what once made
/// pressing down in the box shuffle the bags instead.
///
/// Only downwards. Pressing up on the top row already does something in this
/// game — it moves to the tabs above the panel — and a binding that fights an
/// existing one is worse than none. Reaching earlier slots is done by carrying
/// on down, which wraps around at the end.
fn try_edge_scroll(cursor: Option<i32>) {
    let Some(target) = scroll_target() else { return };

    let cursor = match target {
        Target::Box | Target::Partner(_) => panel::partner_cursor(),
        Target::Bags if panel::phase() == Some(panel::PHASE_SECOND_ITEM) => {
            panel::second_cursor()
        }
        Target::Bags => cursor,
    };

    let Some(cursor) = cursor else { return };

    if cursor < BAG_SIZE as i32 - panel::COLUMNS {
        return;
    }

    if !scroll_by(target, SCROLL_STEP) {
        // Already at the end of the store, so start over from the top.
        if !rewind(target) {
            return;
        }

        log_info!("Wrapped back to the first slot.");
    }

    // The selection stays where it is — the bottom row — and the next items
    // arrive under it. Following the old item upward was tried first, and it
    // reads exactly backwards: the player pressed down to reach the new items,
    // not to keep holding the old one.
    after_scroll(target);
}

/// Whether "down" is being asked for, by keyboard or controller.
///
/// `pad_seen` records the first time a controller reports anything at all, so
/// a controller that is connected but silent can be told apart from one that is
/// not being read correctly.
fn holding_down(controller: &Option<Controller>, pad_seen: &mut bool) -> bool {
    if pressed(VK_DOWN) || pressed(VK_S) {
        return true;
    }

    let Some(pad) = controller.as_ref() else {
        return false;
    };

    let buttons = pad.buttons();
    let stick = pad.left_stick_y();

    if !*pad_seen && (buttons != 0 || stick.unsigned_abs() > STICK_THRESHOLD as u16) {
        *pad_seen = true;
        log_info!("Controller reporting: buttons 0x{buttons:04X}, stick {stick}.");
    }

    buttons & BUTTON_DPAD_DOWN != 0 || stick < -STICK_THRESHOLD
}

fn pressed(key: i32) -> bool {
    let state = unsafe { GetAsyncKeyState(key) };
    state as u16 & KEY_PRESSED != 0
}

fn dispatch_command(key: i32, ini: &Path, debug_keys: bool) {
    match key {
        VK_HOME => toggle_box(),
        VK_PAGE_UP => scroll_focused(-SCROLL_STEP),
        VK_PAGE_DOWN => scroll_focused(SCROLL_STEP),

        // Everything below is diagnostic and stays behind the config switch.
        _ if !debug_keys => {}

        VK_F7 => crate::debug::selection::report(),
        VK_F8 => unsafe {
            crate::hook::remove_all_installed();
            crate::feature::remove_all();
            #[cfg(any(feature = "expanded", feature = "itembox"))]
            crate::save::remove_installed();
        },
        VK_F9 => probe::scan(ini),
        VK_F10 => probe::narrow(ini),
        VK_F11 => probe::inspect(),
        VK_F12 => probe::log_regions(),
        _ => {}
    }
}

/// Reports where each window landed, and what the panel has been told.
fn report() {
    if let Some((position, capacity, empty)) = crate::feature::item_box::state() {
        log_info!(
            "Item box: showing slots {}-{} of {}, {} free.",
            position + 1,
            position + BAG_SIZE,
            capacity,
            empty
        );
    }

    for (bag, position, capacity, empty) in registry::positions() {
        log_info!(
            "Bag 0x{:08X}: showing slots {}-{} of {}, {} free.",
            bag,
            position + 1,
            position + BAG_SIZE,
            capacity,
            empty
        );
    }

    log_debug!(
        "Panel drawn {} time(s); cursor now {:?}.",
        panel::draw_count(),
        panel::cursor()
    );
}

/// Scrolls whatever the selection is over by `rows`, if a scroll is allowed.
fn scroll_focused(rows: i32) {
    let Some(target) = scroll_target() else { return };

    if scroll_by(target, rows) {
        after_scroll(target);
    } else {
        report();
    }
}

/// Scrolls down a row, starting over from the top at the end of the list.
///
/// Shared by the right stick click and by pressing against the bottom row: both
/// mean "show me the next rows", and both need somewhere to go once there are
/// no next rows left.
fn scroll_or_wrap() {
    let Some(target) = scroll_target() else { return };

    if scroll_by(target, SCROLL_STEP) {
        after_scroll(target);
        return;
    }

    if rewind(target) {
        log_info!("Wrapped back to the first slot.");
        after_scroll(target);
    }
}

/// Shows the item box in place of the partner's bag.
///
/// Opening only. An open box stays until the menu closes, so an accidental
/// stick click cannot make it vanish mid-use.
///
/// The panel is asked to redraw: the icons are built when the inventory opens,
/// so swapping what the panel is showing is exactly the case that needs one.
fn toggle_box() {
    crate::feature::item_box::open_from_key();
    panel::request_redraw();
}
