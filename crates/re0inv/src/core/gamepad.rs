//! Reading a controller, without depending on one being there.
//!
//! XInput lives in a DLL whose exact name changes between Windows versions.
//! Linking against it directly would mean this mod fails to load at all on a
//! machine that has a different one, which is a bad trade for an input method
//! that is optional. So it is looked up at runtime and simply absent if it is
//! not there.
//!
//! This reads the controller directly rather than going through the game's own
//! input handling. That is the cruder of the two options: it does not respect
//! the player's button bindings, and it does not know whether a menu is even
//! open. Doing it properly means hooking the game's input, which needs an
//! understanding of the menu that this project does not have yet.

use std::ffi::c_void;

use crate::core::logging::{log_debug, log_info};
use crate::win32::{GetProcAddress, LoadLibraryA};

/// Names to try, newest first. Windows 10 and 11 ship `xinput1_4`.
const CANDIDATES: [&[u8]; 3] = [b"xinput1_4.dll\0", b"xinput1_3.dll\0", b"xinput9_1_0.dll\0"];

/// `XInputGetState` returns this when the pad in that slot is not connected.
const ERROR_DEVICE_NOT_CONNECTED: u32 = 1167;

/// Controllers XInput can report on.
const MAX_PADS: u32 = 4;

/// Thumbstick clicks. The shoulder buttons are already spoken for in this
/// game, and a scroll binding that fights an existing one is worse than none.
pub const BUTTON_LEFT_THUMB: u16 = 0x0040;
pub const BUTTON_RIGHT_THUMB: u16 = 0x0080;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Gamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct State {
    packet: u32,
    gamepad: Gamepad,
}

type XInputGetState = unsafe extern "system" fn(u32, *mut State) -> u32;

pub struct Controller {
    get_state: XInputGetState,
}

impl Controller {
    /// Finds XInput, or `None` if this machine has none of the known versions.
    pub fn load() -> Option<Controller> {
        for name in CANDIDATES {
            let module = unsafe { LoadLibraryA(name.as_ptr()) };
            if module.is_null() {
                continue;
            }

            let symbol = unsafe { GetProcAddress(module, c"XInputGetState".as_ptr() as *const u8) };
            let Some(symbol) = symbol else { continue };

            log_info!(
                "Controller support through {}.",
                String::from_utf8_lossy(&name[..name.len() - 1])
            );

            // Safety: the symbol came from XInput, so it has that signature.
            let get_state: XInputGetState = unsafe { std::mem::transmute(symbol) };
            return Some(Controller { get_state });
        }

        log_debug!("No XInput library found; controller input is off.");
        None
    }

    /// Buttons held on any connected controller.
    ///
    /// The pads are merged rather than tracked separately: which one the player
    /// is holding does not matter, only that a button is down.
    pub fn buttons(&self) -> u16 {
        let mut held = 0;

        for pad in 0..MAX_PADS {
            let mut state = State::default();
            let result = unsafe { (self.get_state)(pad, &mut state) };

            if result == ERROR_DEVICE_NOT_CONNECTED {
                continue;
            }

            held |= state.gamepad.buttons;
        }

        held
    }
}

/// Silences the unused warning for a type only used through a raw pointer.
#[allow(dead_code)]
fn _unused(_: *mut c_void) {}
