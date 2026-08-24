//! The improvements this mod bundles, one module each.
//!
//! Each is independently switchable in the configuration, and each is off unless
//! asked for. They share the patching machinery in `hook` and the verified
//! addresses in `game::addresses`; nothing else is shared, so one feature
//! failing to install leaves the others alone.

pub mod doors;
pub mod item_box;

use std::sync::Mutex;

use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;

/// What has been applied, kept because the bytes each patch replaced live
/// inside it and are the only way back.
static INSTALLED: Mutex<Option<doors::Doors>> = Mutex::new(None);

/// Applies every feature the configuration asks for.
///
/// # Safety
/// The addresses must belong to the build actually running, and the code
/// section must be decrypted.
pub unsafe fn install_all(addresses: &Addresses, config: &Config) {
    if config.item_box.enabled {
        item_box::enable(config.item_box.slots);
    }

    if !config.doors.skip {
        return;
    }

    let doors = doors::Doors::install(addresses, config.doors.shorten_fades);

    match INSTALLED.lock() {
        Ok(mut slot) => *slot = Some(doors),
        Err(_) => log_warn!("Feature registry is poisoned; door patches cannot be undone."),
    }
}

/// Puts back everything the features changed.
///
/// # Safety
/// The game module must still be mapped.
pub unsafe fn remove_all() {
    match INSTALLED.lock() {
        Ok(mut slot) => match slot.as_mut() {
            Some(doors) => doors.remove(),
            None => log_info!("No features are installed."),
        },
        Err(_) => log_warn!("Feature registry is poisoned; refusing to touch the game's code."),
    }
}
