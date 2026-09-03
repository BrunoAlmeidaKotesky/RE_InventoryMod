//! Writes a dump of the process when the game stops responding.
//!
//! Windows records a hang (`AppHangB1`) but keeps no dump of it, and a player
//! staring at a frozen window is not going to open Task Manager and find the
//! right menu. So the mod watches its own process: a thread of its own asks
//! Windows every second whether the game's window has gone unresponsive, and
//! when it has, writes a minidump beside the game and a short report of what
//! this mod's threads and locks were doing.
//!
//! The thread takes none of the mod's locks. If a hang is a deadlock between
//! the game and this mod, a watchdog that needed those locks would hang with
//! them. Lock states are read with `try_lock`, which never waits.

use std::ffi::c_void;
use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use crate::core::logging::{log_info, log_warn};
use crate::win32::{
    EnumWindows, GetCurrentProcess, GetCurrentProcessId, GetProcAddress,
    GetWindowThreadProcessId, Handle, IsHungAppWindow, IsWindowVisible, LoadLibraryA, Bool,
};

const CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Consecutive positive checks before acting. `IsHungAppWindow` already needs
/// five seconds of silence; this rides out a long loading screen on top.
const CONFIRMATIONS: u32 = 3;

/// `MiniDumpWithDataSegs | MiniDumpWithIndirectlyReferencedMemory |
/// MiniDumpWithThreadInfo`: every thread's stack and context, the modules'
/// data, and what the stacks point at. Tens of megabytes, not gigabytes.
const DUMP_TYPE: u32 = 0x0001 | 0x0040 | 0x1000;

type MiniDumpWriteDump = unsafe extern "system" fn(
    process: Handle,
    process_id: u32,
    file: Handle,
    dump_type: u32,
    exception: *const c_void,
    user_stream: *const c_void,
    callback: *const c_void,
) -> Bool;

static START: OnceLock<Instant> = OnceLock::new();
static INPUT_BEAT: AtomicU64 = AtomicU64::new(0);
static DUMPED: AtomicBool = AtomicBool::new(false);

fn uptime_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// The input thread calls this every pass, so the report can say whether it
/// was still running when the game froze.
pub fn beat() {
    INPUT_BEAT.store(uptime_ms(), Ordering::Relaxed);
}

/// A lock's state without waiting for it.
pub fn describe_lock<T>(lock: &Mutex<T>) -> &'static str {
    match lock.try_lock() {
        Ok(_) => "free",
        Err(TryLockError::WouldBlock) => "HELD",
        Err(TryLockError::Poisoned(_)) => "poisoned",
    }
}

pub fn start(dump: PathBuf, report: PathBuf) {
    START.get_or_init(Instant::now);

    let spawned = std::thread::Builder::new()
        .name("re0inv-watchdog".into())
        .spawn(move || watch(&dump, &report));

    if spawned.is_err() {
        log_warn!("Could not start the hang watchdog.");
    }
}

fn watch(dump: &Path, report: &Path) {
    let mut window: Handle = std::ptr::null_mut();
    let mut strikes = 0;

    loop {
        std::thread::sleep(CHECK_INTERVAL);

        // The window appears some time after the mod loads, and may be
        // recreated on a display mode change.
        if window.is_null() || unsafe { IsWindowVisible(window) } == 0 {
            window = main_window();
            if window.is_null() {
                continue;
            }
        }

        if unsafe { IsHungAppWindow(window) } == 0 {
            strikes = 0;
            continue;
        }

        strikes += 1;
        if strikes < CONFIRMATIONS || DUMPED.swap(true, Ordering::Relaxed) {
            continue;
        }

        // Report and dump before logging: the log has a lock of its own, and
        // a thread stuck while holding it is exactly what this is here for.
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        let _ = std::fs::write(report, describe(owner));

        match write_dump(dump) {
            Ok(()) => log_info!("Game unresponsive: dump written to {}.", dump.display()),
            Err(e) => log_warn!("Game unresponsive, and the dump failed: {e}."),
        }
    }
}

/// The first visible top-level window this process owns.
fn main_window() -> Handle {
    unsafe extern "system" fn visit(window: Handle, parameter: isize) -> Bool {
        let mut process = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut process) };

        if process == unsafe { GetCurrentProcessId() } && unsafe { IsWindowVisible(window) } != 0 {
            unsafe { *(parameter as *mut Handle) = window };
            return 0;
        }

        1
    }

    let mut found: Handle = std::ptr::null_mut();
    unsafe { EnumWindows(Some(visit), &mut found as *mut Handle as isize) };
    found
}

fn describe(window_thread: u32) -> String {
    let mut text = String::new();

    text.push_str(&format!(
        "game unresponsive after {} ms; window thread {window_thread}\n",
        uptime_ms()
    ));

    let beat = INPUT_BEAT.load(Ordering::Relaxed);
    if beat == 0 {
        text.push_str("input thread: never ran\n");
    } else {
        text.push_str(&format!(
            "input thread: last pass {} ms ago\n",
            uptime_ms().saturating_sub(beat)
        ));
    }

    for (name, state) in crate::lock_states() {
        text.push_str(&format!("lock {name}: {state}\n"));
    }

    text
}

fn write_dump(path: &Path) -> Result<(), String> {
    let dbghelp = unsafe { LoadLibraryA(c"dbghelp.dll".as_ptr().cast()) };
    if dbghelp.is_null() {
        return Err("dbghelp.dll did not load".into());
    }

    let Some(entry) = (unsafe { GetProcAddress(dbghelp, c"MiniDumpWriteDump".as_ptr().cast()) })
    else {
        return Err("MiniDumpWriteDump not exported".into());
    };

    // Safety: the export has this signature on every Windows that ships it.
    let write: MiniDumpWriteDump = unsafe { std::mem::transmute(entry) };

    let file = File::create(path).map_err(|e| e.to_string())?;

    let ok = unsafe {
        write(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file.as_raw_handle() as Handle,
            DUMP_TYPE,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    if ok == 0 {
        return Err("MiniDumpWriteDump returned false".into());
    }

    Ok(())
}
