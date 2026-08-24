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
use std::time::{Duration, Instant};

use crate::core::gamepad::{Controller, BUTTON_DPAD_DOWN};
use crate::core::logging::{log_debug, log_info};
use crate::debug::probe;
use crate::game::inventory::BAG_SIZE;
use crate::hook::panel;
use crate::store::registry;
use crate::win32::{GetAsyncKeyState, KEY_PRESSED};

const VK_DOWN: i32 = 0x28;
const VK_S: i32 = 0x53;

const VK_PAGE_UP: i32 = 0x21;
const VK_PAGE_DOWN: i32 = 0x22;

const VK_F8: i32 = 0x77;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;

const COMMAND_KEYS: [i32; 7] = [
    VK_PAGE_UP,
    VK_PAGE_DOWN,
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

/// How long after the selection last moved an edge press still counts.
///
/// This stands in for a proper "the inventory is open" signal. To be pressing
/// against the edge, the player had to navigate there, and navigating moves the
/// selection. Walking around the world does not.
const NAVIGATION_WINDOW: Duration = Duration::from_secs(5);

/// One row of two slots per press.
const SCROLL_STEP: i32 = 1;

pub fn run(ini: PathBuf, debug_keys: bool) {
    log_info!("Inventory scrolling: press down on the bottom row; it wraps at the end.");
    log_info!("Page Up and Page Down also scroll it directly.");
    if debug_keys {
        log_info!("Debug keys: F8 remove hooks, F9 scan, F10 narrow, F11 inspect, F12 memory map.");
    }

    let controller = Controller::load();

    let mut command_was_down = [false; COMMAND_KEYS.len()];
    let mut down_was_down = false;
    let mut pad_seen = false;

    let mut last_cursor: Option<i32> = None;
    let mut last_moved = Instant::now();
    let mut last_phase: Option<i32> = None;

    loop {
        for (index, &key) in COMMAND_KEYS.iter().enumerate() {
            let down = pressed(key);

            if down && !command_was_down[index] {
                dispatch_command(key, &ini, debug_keys);
            }
            command_was_down[index] = down;
        }

        // Watching the selection serves two purposes: it says where the cursor
        // is, and the fact that it moved says the player is in the menu.
        let cursor = panel::cursor();
        if cursor != last_cursor {
            if cursor.is_some() {
                last_moved = Instant::now();
            }
            last_cursor = cursor;
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

        let down = holding_down(&controller, &mut pad_seen);

        if down && !down_was_down {
            try_edge_scroll(cursor, last_moved);
        }

        down_was_down = down;

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Scrolls when the selection is pressed against the bottom row.
///
/// Only downwards. Pressing up on the top row already does something in this
/// game — it moves to the tabs above the panel — and a binding that fights an
/// existing one is worse than none. Reaching earlier slots is done by carrying
/// on down, which wraps around at the end.
fn try_edge_scroll(cursor: Option<i32>, last_moved: Instant) {
    let Some(cursor) = cursor else { return };

    if last_moved.elapsed() > NAVIGATION_WINDOW {
        return;
    }

    if cursor < BAG_SIZE as i32 - panel::COLUMNS {
        return;
    }

    if registry::scroll_all(SCROLL_STEP) == 0 {
        // Already at the end of the store, so start over from the top.
        if registry::rewind_all() == 0 {
            return;
        }

        log_info!("Wrapped back to the first slot.");
        panel::request_redraw();
        report();
        return;
    }

    // The contents moved up by a row under the selection, so move the selection
    // down by one to keep it on the same item.
    let followed = cursor - panel::COLUMNS;
    if (0..BAG_SIZE as i32).contains(&followed) {
        unsafe { panel::set_cursor(followed) };
    }

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
        VK_PAGE_UP => {
            if registry::scroll_all(-SCROLL_STEP) > 0 {
                panel::request_redraw();
            }
            report();
        }
        VK_PAGE_DOWN => {
            if registry::scroll_all(SCROLL_STEP) > 0 {
                panel::request_redraw();
            }
            report();
        }

        // Everything below is diagnostic and stays behind the config switch.
        _ if !debug_keys => {}

        VK_F8 => unsafe {
            crate::hook::remove_all_installed();
            crate::feature::remove_all();
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
