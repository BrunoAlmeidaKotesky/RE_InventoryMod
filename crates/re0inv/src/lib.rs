//! RE0 Inventory Expansion - ASI plugin for Resident Evil 0 HD Remaster.
//!
//! It loads, identifies the game module, waits out the Steam DRM decryption,
//! then patches the functions whose addresses have been verified for that exact
//! build. Anything unverified is left alone.

#![cfg(windows)]
// The doors-only build compiles the accessor and store machinery but installs
// none of it; the linker strips what nothing reaches. Warning about each unused
// piece would mean scattering cfg through code the other builds exercise fully.
#![cfg_attr(not(any(feature = "expanded", feature = "itembox")), allow(dead_code))]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::win32::{
    CloseHandle, CreateThread, DisableThreadLibraryCalls, DLL_PROCESS_ATTACH, TRUE,
};

mod core;
#[cfg(any(feature = "expanded", feature = "itembox"))]
mod save;
mod debug;
mod feature;
mod game;
mod hook;
mod store;
mod win32;

use crate::core::config::Config;
use crate::core::logging::{self, log_debug, log_error, log_info, log_warn};
use crate::game::addresses;
use crate::game::build::{self, Build};
use crate::game::module::{self, Module};

const MOD_NAME: &str = "RE0 Inventory Expansion";
const MOD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Generous ceiling; decryption normally finishes in well under a second.
const DECRYPT_TIMEOUT: Duration = Duration::from_secs(60);

/// DLL entry point.
///
/// Runs while the loader lock is held, so it does nothing but spawn a thread.
/// Any I/O or waiting here risks deadlocking process startup.
///
/// # Safety
/// Called by the Windows loader with a valid module handle.
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    instance: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            // The game spawns many threads and we care about none of them.
            DisableThreadLibraryCalls(instance);

            let thread = CreateThread(
                std::ptr::null(),
                0,
                Some(main_thread),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
            );

            if !thread.is_null() {
                CloseHandle(thread);
            }
        }
    }

    // Always report success: a failed mod must not stop the game from starting.
    TRUE
}

unsafe extern "system" fn main_thread(_param: *mut c_void) -> u32 {
    // A panic unwinding across the FFI boundary is undefined behaviour.
    // Catching it means the worst case is a dead mod, not a dead game.
    if std::panic::catch_unwind(startup).is_err() {
        log_error!("Mod panicked. No hooks remain active.");
    }
    0
}

fn startup() {
    let game_dir = module::game_directory();
    let ini_path = game_dir.join("re0inv.ini");

    let mut config = Config::load(&ini_path);
    logging::init(&resolve(&game_dir, &config.log_path), config.log_level);

    log_info!("{} v{}", MOD_NAME, MOD_VERSION);
    log_info!("Game directory: {}", game_dir.display());

    for warning in config.sanitize() {
        log_warn!("{}", warning);
    }

    if !config.enabled {
        log_info!("Disabled by configuration (Mod=0). Nothing will be modified.");
        return;
    }

    let Some(module) = Module::current_process() else {
        log_error!("Could not read the process PE header. Aborting.");
        return;
    };
    log_module(&module);

    let Some(code) = module.code() else {
        log_error!("No code section found. Aborting.");
        return;
    };

    if module.is_drm_packed() {
        log_info!("Section .bind present: the executable is packed by Steam DRM.");
        if !debug::dump::wait_until_decrypted(code, DECRYPT_TIMEOUT) {
            log_error!("Code section never became readable. Aborting.");
            return;
        }
    }

    let detected = Build::detect(&module);
    match &detected {
        Some(b) if b.is_supported() => log_info!("Game build: {}", b.stamp),
        Some(b) => {
            log_warn!("Game build: {}", b.stamp);
            log_warn!("Expected {}. Known addresses may not apply.", build::SUPPORTED);
        }
        None => log_warn!("Could not identify the game build."),
    }

    log_info!("Configured inventory slots: {}.", config.slots);

    if config.debug.dump_text {
        let path = resolve(&game_dir, &config.debug.dump_path);
        if let Err(e) = debug::dump::dump_section(code, &path) {
            log_error!("Failed to write the dump: {}", e);
        }
    }

    // Without the expanded-inventory feature the stores stay at the game's own
    // six slots: the views pass straight through, and only the box, if built
    // in, makes use of the machinery.
    #[cfg(feature = "expanded")]
    store::registry::set_capacity(config.slots);
    #[cfg(not(feature = "expanded"))]
    store::registry::set_capacity(game::inventory::BAG_SIZE);

    // The accessor and panel hooks serve both the extra slots and the box; a
    // doors-only build patches neither, and neither needs saving.
    #[cfg(any(feature = "expanded", feature = "itembox"))]
    install_hooks(detected.as_ref());

    // The prompt archives are built here, from the player's own files, so a
    // fresh install works without any tool being run first.
    #[cfg(feature = "itembox")]
    if config.item_box.enabled {
        feature::typewriter::message::ensure_archives(&game_dir);
    }

    install_features(detected.as_ref(), &config);

    // Last, and always. Everything above only matters while the process is
    // running; this is what stops the player losing it when they stop playing.
    #[cfg(any(feature = "expanded", feature = "itembox"))]
    install_persistence(detected.as_ref(), &game_dir, code.start..code.end());

    log_info!("Initialization complete.");

    // Takes over this thread; nothing runs after it. It always runs, because
    // scrolling the inventory is read from the keyboard here.
    core::input::run(ini_path, config.debug.probe);
}

/// Patches the game, but only for a build whose addresses were verified.
///
/// Writing a jump into whatever happens to sit at an address in some other
/// build is how a mod corrupts a save. Doing nothing is always the safer
/// failure.
#[cfg(any(feature = "expanded", feature = "itembox"))]
fn install_hooks(build: Option<&Build>) {
    let Some(build) = build else {
        log_warn!("Build unknown, so nothing will be patched.");
        return;
    };

    let Some(addresses) = addresses::for_build(build) else {
        log_warn!("No verified addresses for this build, so nothing will be patched.");
        return;
    };

    unsafe { hook::install_and_keep(&addresses) };
}

/// Applies the optional improvements, each only if it was asked for.
///
/// Kept apart from the inventory hooks: those are what the mod is, and these are
/// extras. One of them failing, or being switched off, must leave the rest of
/// the mod exactly as it was.
fn install_features(build: Option<&Build>, config: &Config) {
    let Some(addresses) = build.and_then(addresses::for_build) else {
        if config.doors.skip {
            log_warn!("Door skip is on, but this build has no verified addresses for it.");
        }
        return;
    };

    unsafe { feature::install_all(&addresses, config) };
}

/// Hooks saving and loading, so the extra slots outlive the process.
///
/// The file sits beside the game rather than inside the save. Nothing here can
/// damage a save file, and that is the entire reason it was built this way.
///
/// This is not behind a switch. Turning the extra slots off while a save has
/// items in them is exactly when the data has to be read and written back
/// untouched, rather than quietly dropped.
#[cfg(any(feature = "expanded", feature = "itembox"))]
fn install_persistence(build: Option<&Build>, game_dir: &Path, code: std::ops::Range<usize>) {
    let Some(addresses) = build.and_then(addresses::for_build) else {
        log_warn!("No verified addresses for this build, so nothing will be saved or loaded.");
        return;
    };

    let path = game_dir.join("re0inv_saves.bin");
    unsafe { save::Persistence::install(&addresses, path, code) };
}

fn log_module(module: &Module) {
    log_info!(
        "Game module: base 0x{:08X}, size 0x{:08X}, {} sections.",
        module.base,
        module.size,
        module.sections.len()
    );

    for s in &module.sections {
        log_info!(
            "  {:<8} 0x{:08X} - 0x{:08X}  {}{}",
            s.name,
            s.start,
            s.end(),
            if s.executable { "X" } else { "-" },
            if s.writable { "W" } else { "-" }
        );
    }

    log_debug!("Module walk complete.");
}

/// Relative config paths are anchored to the game directory.
fn resolve(game_dir: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        game_dir.join(candidate)
    }
}
