//! Minimal file logger.

use std::fmt::Arguments;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub fn from_str(s: &str) -> Level {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Level::Off,
            "error" => Level::Error,
            "warn" | "warning" => Level::Warn,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Info,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Level::Off => "OFF",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

struct Logger {
    file: Mutex<Option<File>>,
    level: Level,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn init(path: &Path, level: Level) {
    let file = File::create(path).ok();
    let _ = LOGGER.set(Logger { file: Mutex::new(file), level });
}

pub fn write(level: Level, args: Arguments) {
    let Some(logger) = LOGGER.get() else { return };
    if logger.level == Level::Off || level > logger.level {
        return;
    }

    let mut guard = match logger.file.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "[{}] {}", level.tag(), args);
        // Flush per line: a crash must not lose the last entry.
        let _ = file.flush();
    }
}

macro_rules! log_error { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Error, format_args!($($a)*)) } }
macro_rules! log_warn  { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Warn,  format_args!($($a)*)) } }
macro_rules! log_info  { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Info,  format_args!($($a)*)) } }
macro_rules! log_debug { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Debug, format_args!($($a)*)) } }

pub(crate) use {log_debug, log_error, log_info, log_warn};
