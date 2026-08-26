//! The improvements this mod bundles, one module each.
//!
//! Each is independently switchable in the configuration, and each is off unless
//! asked for. They share the patching machinery in `hook` and the verified
//! addresses in `game::addresses`; nothing else is shared, so one feature
//! failing to install leaves the others alone.
//!
//! On top of the runtime switches, each feature is a Cargo feature, so the mod
//! also ships as single-feature builds. A build without a feature carries a
//! stub of its public surface — `is_open` answering no, `enable` explaining
//! itself — so the rest of the code neither knows nor cares which build it is.

#[cfg(feature = "doors")]
pub mod doors;
#[cfg(feature = "itembox")]
pub mod item_box;
#[cfg(feature = "itembox")]
pub mod menu;
#[cfg(feature = "itembox")]
pub mod typewriter;

/// What the rest of the mod may ask about a box that was not compiled in.
///
/// Every answer is the inert one, so the callers need no conditions of their
/// own: nothing is ever open, storing succeeds at storing nothing, and the one
/// answer a player could notice — the toggle key — says what is going on.
#[cfg(not(feature = "itembox"))]
#[allow(dead_code)] // Which stubs are called depends on the other features.
pub mod item_box {
    use crate::core::logging::log_warn;
    use crate::game::inventory::{Bag, Item};
    use crate::store::window::Window;

    pub fn is_open() -> bool {
        false
    }

    pub fn exists() -> bool {
        false
    }

    pub fn open_from_key() -> bool {
        log_warn!("This build of the mod does not include the item box.");
        false
    }

    pub fn abandon() {}

    pub fn close_with_menu() {}

    pub fn scroll(_rows: i32) -> bool {
        false
    }

    pub fn state() -> Option<(usize, usize, usize)> {
        None
    }

    pub fn contents() -> Vec<Item> {
        Vec::new()
    }

    pub fn set_contents(_items: Vec<Item>) {}

    pub fn view() -> *mut Bag {
        std::ptr::null_mut()
    }

    pub fn with_window<R>(_view: *const Bag, _action: impl FnOnce(&mut Window) -> R) -> Option<R> {
        None
    }

    pub fn probe_positions(_view: *const Bag, _ask: impl FnMut() -> i32) -> Option<i32> {
        None
    }
}

use std::sync::Mutex;

use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;

/// What has been applied, kept because the bytes each patch replaced live
/// inside it and are the only way back.
static INSTALLED: Mutex<Installed> = Mutex::new(Installed {
    #[cfg(feature = "doors")]
    doors: None,
    #[cfg(feature = "itembox")]
    typewriter: None,
    #[cfg(feature = "itembox")]
    menu: None,
});

#[derive(Default)]
struct Installed {
    #[cfg(feature = "doors")]
    doors: Option<doors::Doors>,
    #[cfg(feature = "itembox")]
    typewriter: Option<typewriter::Typewriter>,
    #[cfg(feature = "itembox")]
    menu: Option<menu::Menu>,
}

/// Applies every feature the configuration asks for.
///
/// # Safety
/// The addresses must belong to the build actually running, and the code
/// section must be decrypted.
pub unsafe fn install_all(#[cfg_attr(not(any(feature = "itembox", feature = "doors")), allow(unused_variables))] addresses: &Addresses, config: &Config) {
    #[cfg(feature = "itembox")]
    if config.item_box.enabled {
        item_box::enable(config.item_box.slots);
        watch_typewriter(addresses);

        let prompt = typewriter::Typewriter::install(addresses);
        keep(|installed| installed.typewriter = Some(prompt));

        // The screen itself has to agree that the partner half is part of it.
        // Without this the box opens into the single-panel layout the game
        // shows whenever the two characters are apart.
        let screen = menu::Menu::install(addresses);
        keep(|installed| installed.menu = Some(screen));
    }

    #[cfg(not(feature = "itembox"))]
    if config.item_box.enabled {
        log_warn!("ItemBox=1 in the ini, but this build does not include the item box.");
    }

    #[cfg(feature = "doors")]
    if config.doors.skip {
        let doors = doors::Doors::install(addresses, config.doors.shorten_fades);
        keep(|installed| installed.doors = Some(doors));
    }

    #[cfg(not(feature = "doors"))]
    if config.doors.skip {
        log_warn!("SkipDoors=1 in the ini, but this build does not include the door skip.");
    }
}

#[cfg(any(feature = "itembox", feature = "doors"))]
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
    // With the hooks gone nothing ever closes the box, and an accessor still
    // patched in mid-removal would keep answering with it out in the world.
    item_box::abandon();

    #[cfg_attr(not(any(feature = "itembox", feature = "doors")), allow(unused_mut, unused_variables))]
    let Ok(mut installed) = INSTALLED.lock() else {
        log_warn!("Feature registry is poisoned; refusing to touch the game's code.");
        return;
    };

    #[cfg_attr(not(any(feature = "itembox", feature = "doors")), allow(unused_mut))]
    let mut removed = 0;

    #[cfg(feature = "itembox")]
    if let Some(screen) = installed.menu.as_mut() {
        screen.remove();
        removed += 1;
    }

    #[cfg(feature = "itembox")]
    if let Some(prompt) = installed.typewriter.as_mut() {
        prompt.remove();
        removed += 1;
    }

    #[cfg(feature = "doors")]
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
#[cfg(feature = "itembox")]
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
