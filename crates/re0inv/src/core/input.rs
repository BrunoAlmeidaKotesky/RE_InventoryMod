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
use std::time::Duration;

use crate::core::gamepad::{
    Controller, BUTTON_DPAD_DOWN, BUTTON_LEFT_THUMB, BUTTON_RIGHT_THUMB,
};
use crate::core::logging::{log_debug, log_info};
use crate::debug::probe;
use crate::game::inventory::BAG_SIZE;
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
        }
        menu_was_open = menu_open;

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

/// Scrolls when the selection is pressed against the bottom row.
///
/// Which half the selection is in decides both which cursor to read and what
/// to scroll. In the partner's half — which is the box, when it is showing —
/// the game keeps a separate cursor, and the played half's freezes at its last
/// value; reading the frozen one here is what once made pressing down in the
/// box shuffle the bags instead.
///
/// Only downwards. Pressing up on the top row already does something in this
/// game — it moves to the tabs above the panel — and a binding that fights an
/// existing one is worse than none. Reaching earlier slots is done by carrying
/// on down, which wraps around at the end.
fn try_edge_scroll(cursor: Option<i32>) {
    if !panel::is_open() {
        return;
    }

    let in_partner_half = panel::phase() == Some(panel::PHASE_PARTNER_HALF);

    let cursor = if in_partner_half {
        panel::partner_cursor()
    } else {
        cursor
    };

    let Some(cursor) = cursor else { return };

    if cursor < BAG_SIZE as i32 - panel::COLUMNS {
        return;
    }

    let box_focused = in_partner_half && crate::feature::item_box::is_open();

    let moved = if box_focused {
        crate::feature::item_box::scroll(SCROLL_STEP)
    } else {
        registry::scroll_all(SCROLL_STEP) > 0
    };

    if !moved {
        // Already at the end of the store, so start over from the top.
        let wrapped = if box_focused {
            crate::feature::item_box::scroll(i32::MIN / 2)
        } else {
            registry::rewind_all() > 0
        };

        if !wrapped {
            return;
        }

        log_info!("Wrapped back to the first slot.");
    }

    // The selection stays where it is — the bottom row — and the next items
    // arrive under it. Following the old item upward was tried first, and it
    // reads exactly backwards: the player pressed down to reach the new items,
    // not to keep holding the old one.
    panel::request_redraw();
    report();
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
        VK_PAGE_UP => {
            if scroll_focused(-SCROLL_STEP) > 0 {
                panel::request_redraw();
            }
            report();
        }
        VK_PAGE_DOWN => {
            if scroll_focused(SCROLL_STEP) > 0 {
                panel::request_redraw();
            }
            report();
        }

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

/// Scrolls whichever storage the player is looking at.
///
/// With the box open it scrolls the box and nothing else. Scrolling the bags
/// underneath at the same time read as the two characters' inventories
/// shuffling while the box sat still — which is exactly how it was reported.
///
/// Returns how many windows moved, so the caller can tell "nothing moved
/// because everything is already at the end" from "nothing to scroll".
fn scroll_focused(rows: i32) -> usize {
    if crate::feature::item_box::is_open() {
        return usize::from(crate::feature::item_box::scroll(rows));
    }

    registry::scroll_all(rows)
}

/// Scrolls down a row, starting over from the top at the end of the list.
///
/// Shared by the right stick click and by pressing against the bottom row: both
/// mean "show me the next rows", and both need somewhere to go once there are
/// no next rows left.
fn scroll_or_wrap() {
    if scroll_focused(SCROLL_STEP) > 0 {
        panel::request_redraw();
        report();
        return;
    }

    let wrapped = if crate::feature::item_box::is_open() {
        crate::feature::item_box::scroll(i32::MIN / 2)
    } else {
        registry::rewind_all() > 0
    };

    if wrapped {
        log_info!("Wrapped back to the first slot.");
        panel::request_redraw();
        report();
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
