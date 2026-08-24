//! RE0 Inventory Expansion - ASI plugin for Resident Evil 0 HD Remaster.
//!
//! Phase 1: load, identify the module, wait out the Steam DRM decryption,
//! optionally dump the code section and run the memory probe. No hooks yet.

#![cfg(windows)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::LibraryLoader::DisableThreadLibraryCalls;
use windows_sys::Win32::System::Threading::CreateThread;

mod config;
mod dump;
mod log;
mod pe;
mod probe;

use config::Config;
use log::{log_debug, log_error, log_info, log_warn};

const MOD_NAME: &str = "RE0 Inventory Expansion";
const MOD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the known addresses were gathered against.
const SUPPORTED_BUILD: &str = "MasterRelease Jan 28 2025 16:45:59";

const DLL_PROCESS_ATTACH: u32 = 1;
const DECRYPT_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs under the loader lock, so it only spawns a thread and returns.
#[no_mangle]
pub extern "system" fn DllMain(instance: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            DisableThreadLibraryCalls(instance);

            let handle = CreateThread(
                std::ptr::null(),
                0,
                Some(mod_thread),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            );

            if !handle.is_null() {
                CloseHandle(handle);
            }
        }
    }

    1
}

unsafe extern "system" fn mod_thread(_param: *mut c_void) -> u32 {
    // A panic crossing the FFI boundary is undefined behaviour. Worst case the
    // mod dies quietly and the game keeps running.
    if std::panic::catch_unwind(run).is_err() {
        log_error!("Mod panicked. No hooks remain active.");
    }
    0
}

fn run() {
    let game_dir = pe::game_directory();
    let ini = game_dir.join("re0inv.ini");

    let mut cfg = Config::load(&ini);
    log::init(&resolve(&game_dir, &cfg.log_path), cfg.log_level);

    log_info!("{} v{}", MOD_NAME, MOD_VERSION);
    log_info!("Game directory: {}", game_dir.display());

    for w in cfg.sanitize() {
        log_warn!("{}", w);
    }

    if !cfg.enabled {
        log_info!("Disabled by configuration (Mod=0). Nothing will be modified.");
        return;
    }

    let Some(module) = pe::Module::current_process() else {
        log_error!("Could not read the process PE header. Aborting.");
        return;
    };

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

    let Some(code) = module.code_section() else {
        log_error!("No code section found. Aborting.");
        return;
    };

    // `.bind` is the Steam DRM packer's signature.
    if module.section(".bind").is_some() {
        log_info!("Section .bind present: executable is packed by Steam DRM.");
        if !dump::wait_until_decrypted(code, DECRYPT_TIMEOUT) {
            log_error!("Code section never became readable. Aborting.");
            return;
        }
    }

    match detect_build(&module) {
        Some(build) => {
            log_info!("Game build: {}", build);
            if build != SUPPORTED_BUILD {
                log_warn!("Unsupported build; known addresses may not apply.");
            }
        }
        None => log_warn!("Could not identify the game build."),
    }

    log_info!("Configured inventory slots: {}.", cfg.slots);
    log_debug!("Probe: {}, dump_text: {}", cfg.probe, cfg.dump_text);

    if cfg.dump_text {
        let path = resolve(&game_dir, &cfg.dump_path);
        if let Err(e) = dump::dump_section(code, &path) {
            log_error!("Failed to write the dump: {}", e);
        }
    }

    log_info!("Initialization complete. No hooks installed in this phase.");

    if cfg.probe {
        probe::run(ini);
    }
}

/// Version string the game keeps in read-only data. Never encrypted, unlike `.text`.
fn detect_build(module: &pe::Module) -> Option<String> {
    const MARKER: &[u8] = b"MasterRelease ";

    let rdata = module.section(".rdata")?;
    let offset = pe::find_bytes(unsafe { rdata.as_slice() }, MARKER)?;
    let s = unsafe { pe::read_cstr(rdata.start + offset, 64) };

    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Relative paths are anchored to the game directory.
fn resolve(game_dir: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        game_dir.join(p)
    }
}
