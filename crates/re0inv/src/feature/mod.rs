//! The improvements this mod bundles, one module each.
//!
//! Each is independently switchable in the configuration, and each is off unless
//! asked for. They share the patching machinery in `hook` and the verified
//! addresses in `game::addresses`; nothing else is shared, so one feature
//! failing to install leaves the others alone.

pub mod doors;
pub mod item_box;
pub mod typewriter;

use std::sync::Mutex;

use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;

/// What has been applied, kept because the bytes each patch replaced live
/// inside it and are the only way back.
static INSTALLED: Mutex<Installed> = Mutex::new(Installed {
    doors: None,
    typewriter: None,
});

#[derive(Default)]
struct Installed {
    doors: Option<doors::Doors>,
    typewriter: Option<typewriter::Typewriter>,
}

/// Applies every feature the configuration asks for.
///
/// # Safety
/// The addresses must belong to the build actually running, and the code
/// section must be decrypted.
pub unsafe fn install_all(addresses: &Addresses, config: &Config) {
    if config.item_box.enabled {
        item_box::enable(config.item_box.slots);
        watch_typewriter(addresses);

        let prompt = typewriter::Typewriter::install(addresses);
        keep(|installed| installed.typewriter = Some(prompt));
    }

    if config.doors.skip {
        let doors = doors::Doors::install(addresses, config.doors.shorten_fades);
        keep(|installed| installed.doors = Some(doors));
    }
}

fn keep(action: impl FnOnce(&mut Installed)) {
    match INSTALLED.lock() {
        Ok(mut installed) => action(&mut installed),
        Err(_) => log_warn!("Feature registry is poisoned; these patches cannot be undone."),
    }
}

/// Puts back everything the features changed.
///
/// # Safety
/// The game module must still be mapped.
pub unsafe fn remove_all() {
    let Ok(mut installed) = INSTALLED.lock() else {
        log_warn!("Feature registry is poisoned; refusing to touch the game's code.");
        return;
    };

    let mut removed = 0;

    if let Some(prompt) = installed.typewriter.as_mut() {
        prompt.remove();
        removed += 1;
    }

    if let Some(doors) = installed.doors.as_mut() {
        doors.remove();
        removed += 1;
    }

    if removed == 0 {
        log_info!("No features are installed.");
    }
}

/// Watches for the player being at a typewriter.
///
/// The prompt is the way in, and this is only a fallback: it is what makes the
/// key binding refuse away from a machine, and it says in the log that a
/// typewriter was reached at all, which is worth having when the prompt does
/// not appear.
///
/// # Safety
/// The address must belong to the build actually running.
unsafe fn watch_typewriter(addresses: &Addresses) {
    // `sub esp, 0x54; push esi; push edi` — exactly five bytes, so the jump
    // covers the three instructions with nothing left over to pad.
    const PROLOGUE: [u8; 5] = [0x83, 0xEC, 0x54, 0x56, 0x57];

    item_box::set_typewriter_continue(addresses.typewriter_continue);

    let Some(jump) = crate::hook::detour::jump_bytes(
        addresses.typewriter,
        item_box::typewriter_stub as unsafe extern "C" fn() as usize,
    ) else {
        log_warn!("Typewriter watcher is out of jump range.");
        return;
    };

    match crate::hook::patch::Patch::write_expecting(addresses.typewriter, &PROLOGUE, &jump) {
        Some(patch) => {
            // Leaked on purpose, like the door patches: nothing removes these
            // yet, and the bytes they replaced live inside them.
            std::mem::forget(patch);
            log_info!("Watching for typewriters.");
        }
        None => log_warn!("Could not watch for typewriters."),
    }
}
