//! Writes a dump of the process when the game stops responding.
//!
//! Windows records a hang (`AppHangB1`) but keeps no dump of it, and a player
//! staring at a frozen window is not going to open Task Manager and find the
//! right menu. So the mod watches its own process: a thread of its own asks
//! Windows every second whether the game's window has gone unresponsive, and
//! when it has, writes a minidump beside the game and a short report of what
//! this mod's threads and locks were doing.
//!
//! The thread takes none of the mod's locks, and nothing else that a stuck
//! thread might hold: `dbghelp` is loaded and resolved up front, so no loader
//! lock is needed at the moment that matters, and the outcome goes to the
//! report file rather than the log. Lock states are read with `try_lock`,
//! which never waits.

use std::ffi::c_void;
use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use crate::core::logging::log_warn;
use crate::win32::{
    Bool, EnumWindows, GetClientRect, GetCurrentProcess, GetCurrentProcessId, GetProcAddress,
    GetWindowTextA, GetWindowThreadProcessId, Handle, IsHungAppWindow, IsWindowVisible,
    LoadLibraryA, Rect,
};

const CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Consecutive positive checks before acting. `IsHungAppWindow` already needs
/// five seconds of silence; this rides out a long loading screen on top, so
/// the dump is taken about ten seconds into a freeze.
const CONFIRMATIONS: u32 = 5;

/// `MiniDumpWithIndirectlyReferencedMemory | MiniDumpWithThreadInfo`: every
/// thread's stack and context, and what the stacks point at. Tens of
/// megabytes at most, so the write itself stays short.
const DUMP_TYPE: u32 = 0x0040 | 0x1000;

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

/// `MiniDumpWriteDump`, resolved before any hang can happen.
static WRITE_DUMP: AtomicUsize = AtomicUsize::new(0);

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

    match resolve_writer() {
        Ok(writer) => WRITE_DUMP.store(writer, Ordering::Relaxed),
        Err(e) => log_warn!("Hang dumps unavailable: {e}."),
    }

    let spawned = std::thread::Builder::new()
        .name("re0inv-watchdog".into())
        .spawn(move || watch(&dump, &report));

    if spawned.is_err() {
        log_warn!("Could not start the hang watchdog.");
    }
}

/// Loads `dbghelp` and finds its dump writer, on the mod's own startup thread.
fn resolve_writer() -> Result<usize, String> {
    let dbghelp = unsafe { LoadLibraryA(c"dbghelp.dll".as_ptr().cast()) };
    if dbghelp.is_null() {
        return Err("dbghelp.dll did not load".into());
    }

    let entry = unsafe { GetProcAddress(dbghelp, c"MiniDumpWriteDump".as_ptr().cast()) };
    entry
        .map(|function| function as usize)
        .ok_or_else(|| "MiniDumpWriteDump not exported".into())
}

fn watch(dump: &Path, report: &Path) {
    let mut strikes = 0;

    loop {
        std::thread::sleep(CHECK_INTERVAL);

        // Chosen afresh every time: the window appears some time after the
        // mod loads, may be recreated on a display mode change, and a helper
        // window that never pumps messages must not be mistaken for it.
        let window = main_window();
        if window.is_null() {
            continue;
        }

        if unsafe { IsHungAppWindow(window) } == 0 {
            strikes = 0;
            continue;
        }

        strikes += 1;
        if strikes < CONFIRMATIONS || DUMPED.swap(true, Ordering::Relaxed) {
            continue;
        }

        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        let mut text = describe(window, owner);

        match write_dump(dump) {
            Ok(()) => text.push_str(&format!("dump written to {}\n", dump.display())),
            Err(e) => text.push_str(&format!("dump failed: {e}\n")),
        }

        let _ = std::fs::write(report, text);
    }
}

/// The largest visible top-level window this process owns.
///
/// Largest rather than first: the game's own window fills the screen, and
/// anything else visible in the process — an overlay's helper, a splash — is
/// small, and its thread may never pump messages at all.
fn main_window() -> Handle {
    struct Best {
        window: Handle,
        area: i64,
    }

    unsafe extern "system" fn visit(window: Handle, parameter: isize) -> Bool {
        let mut process = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut process) };

        if process != unsafe { GetCurrentProcessId() } || unsafe { IsWindowVisible(window) } == 0 {
            return 1;
        }

        let mut rect = Rect::default();
        if unsafe { GetClientRect(window, &mut rect) } == 0 {
            return 1;
        }

        let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
        let best = unsafe { &mut *(parameter as *mut Best) };
        if area > best.area {
            best.window = window;
            best.area = area;
        }

        1
    }

    let mut best = Best {
        window: std::ptr::null_mut(),
        area: -1,
    };
    unsafe { EnumWindows(Some(visit), &mut best as *mut Best as isize) };
    best.window
}

fn window_title(window: Handle) -> String {
    let mut buffer = [0u8; 128];
    let length = unsafe { GetWindowTextA(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf8_lossy(&buffer[..length]).into_owned()
}

fn describe(window: Handle, window_thread: u32) -> String {
    let mut text = String::new();

    text.push_str(&format!(
        "game unresponsive after {} ms; window 0x{:08X} \"{}\" on thread {window_thread}\n",
        uptime_ms(),
        window as usize,
        window_title(window)
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
    let writer = WRITE_DUMP.load(Ordering::Relaxed);
    if writer == 0 {
        return Err("MiniDumpWriteDump was not resolved at startup".into());
    }

    // Safety: the value came from GetProcAddress for this exact export, which
    // has this signature on every Windows that ships it.
    let write: MiniDumpWriteDump = unsafe { std::mem::transmute(writer) };

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
