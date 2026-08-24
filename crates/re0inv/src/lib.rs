//! RE0 Inventory Expansion - ASI plugin for Resident Evil 0 HD Remaster.
//!
//! Phase 1: load, identify the game module, wait out the Steam DRM decryption,
//! and optionally run the debug tooling. No hooks are installed yet.

#![cfg(windows)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::win32::{
    CloseHandle, CreateThread, DisableThreadLibraryCalls, DLL_PROCESS_ATTACH, TRUE,
};

mod core;
mod debug;
mod game;
mod win32;

use crate::core::config::Config;
use crate::core::logging::{self, log_debug, log_error, log_info, log_warn};
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
#[no_mangle]
pub extern "system" fn DllMain(instance: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
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

    match Build::detect(&module) {
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

    log_info!("Initialization complete. No hooks installed in this phase.");

    // Takes over this thread; nothing runs after it.
    if config.debug.probe {
        debug::probe::run(ini_path);
    }
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
