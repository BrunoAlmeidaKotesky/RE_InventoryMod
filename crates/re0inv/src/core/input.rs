//! Reading the player's input directly, on the mod's own thread.
//!
//! Scrolling the inventory ought to happen when the cursor is pushed past the
//! edge of the panel, using the game's own navigation. That needs the menu code
//! understood well enough to intervene in it, and it is not yet. Until then the
//! window moves on a dedicated button, read here.
//!
//! The cost of doing it this way is that the mod does not know whether a menu is
//! even open, and does not follow the player's own button bindings. The benefit
//! is that it works today, on keyboard and controller both.

use std::path::PathBuf;
use std::time::Duration;

use crate::core::gamepad::{Controller, BUTTON_LEFT_THUMB, BUTTON_RIGHT_THUMB};
use crate::core::logging::log_info;
use crate::debug::probe;
use crate::game::inventory::BAG_SIZE;
use crate::store::registry;
use crate::win32::{GetAsyncKeyState, KEY_PRESSED};

const VK_PAGE_UP: i32 = 0x21;
const VK_PAGE_DOWN: i32 = 0x22;

const VK_F8: i32 = 0x77;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;

const KEYS: [i32; 7] = [
    VK_PAGE_UP,
    VK_PAGE_DOWN,
    VK_F8,
    VK_F9,
    VK_F10,
    VK_F11,
    VK_F12,
];

const POLL_INTERVAL: Duration = Duration::from_millis(60);

/// One row of two slots per press. Anything larger skips over items.
const SCROLL_STEP: i32 = 1;

/// Polls input forever. Runs on its own thread.
///
/// `debug_keys` gates the diagnostic commands. Scrolling is a feature and is
/// always available; the probe is a tool and is not.
pub fn run(ini: PathBuf, debug_keys: bool) {
    log_info!(
        "Inventory scrolling: Page Up and Page Down, or clicking the left and right sticks."
    );
    if debug_keys {
        log_info!("Debug keys: F8 remove hooks, F9 scan, F10 narrow, F11 inspect, F12 memory map.");
    }

    let controller = Controller::load();

    let mut key_was_down = [false; KEYS.len()];
    let mut pad_was_down: u16 = 0;

    loop {
        for (index, &key) in KEYS.iter().enumerate() {
            let down = unsafe { GetAsyncKeyState(key) } as u16 & KEY_PRESSED != 0;

            // Edge trigger: act once per press, not once per poll.
            if down && !key_was_down[index] {
                dispatch_key(key, &ini, debug_keys);
            }
            key_was_down[index] = down;
        }

        if let Some(controller) = &controller {
            let buttons = controller.buttons();
            let pressed = buttons & !pad_was_down;
            pad_was_down = buttons;

            if pressed & BUTTON_LEFT_THUMB != 0 {
                scroll(-SCROLL_STEP);
            }
            if pressed & BUTTON_RIGHT_THUMB != 0 {
                scroll(SCROLL_STEP);
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

fn dispatch_key(key: i32, ini: &std::path::Path, debug_keys: bool) {
    match key {
        VK_PAGE_UP => scroll(-SCROLL_STEP),
        VK_PAGE_DOWN => scroll(SCROLL_STEP),

        // Everything below is diagnostic and stays behind the config switch.
        _ if !debug_keys => {}

        VK_F8 => unsafe { crate::hook::remove_all_installed() },
        VK_F9 => probe::scan(ini),
        VK_F10 => probe::narrow(ini),
        VK_F11 => probe::inspect(),
        VK_F12 => probe::log_regions(),
        _ => {}
    }
}

/// Moves every inventory window and reports where each one landed.
fn scroll(rows: i32) {
    if registry::scroll_all(rows) == 0 {
        log_info!("Nothing to scroll.");
        return;
    }

    // The panel is built when the inventory opens, not redrawn each frame, so
    // a scroll only shows after closing and reopening. Reporting what is known
    // about the drawing is what a fix for that will be built on.
    match crate::hook::panel::menu() {
        Some(menu) => log_info!(
            "Menu 0x{:08X}, drawn {} time(s) so far.",
            menu,
            crate::hook::panel::draw_count()
        ),
        None => log_info!("The panel has not been drawn yet."),
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
}
