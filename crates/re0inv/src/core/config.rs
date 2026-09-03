//! `re0inv.ini` reader.
//!
//! Flat `key=value` lookup: section headers are skipped and keys are unique
//! across the whole file. `;` and `#` start a comment.

use std::collections::HashMap;
use std::path::Path;

use crate::core::logging::Level;
use crate::game::inventory::BAG_SIZE;

/// Slots per character when the ini says nothing. Double the stock six.
pub const DEFAULT_SLOTS: usize = 12;

/// Slots in the item box when the ini says nothing.
pub const DEFAULT_BOX_SLOTS: usize = 24;

/// Upper bound on slots. Exists to turn a typo into a warning instead of an
/// absurd allocation.
pub const MAX_SLOTS: usize = 32;

#[derive(Clone)]
pub struct Config {
    pub enabled: bool,
    pub log_level: Level,
    pub log_path: String,
    /// Inventory slots per character. Must be even: the game lays the inventory
    /// out in rows of 2, and two-slot items break on an odd count.
    pub slots: usize,
    pub doors: DoorsConfig,
    pub item_box: ItemBoxConfig,
    pub debug: DebugConfig,
}

/// Storage shown in place of the partner's bag.
#[derive(Clone)]
pub struct ItemBoxConfig {
    pub enabled: bool,
    /// How much the box holds. Even, for the same reason the inventory is.
    pub slots: usize,
}

/// Skipping the animation played when walking through a door.
#[derive(Clone)]
pub struct DoorsConfig {
    pub skip: bool,
    /// Also shorten the fades around the transition. Without this the wait is
    /// gone but the screen still fades out and back in around a glimpse of the
    /// door.
    pub shorten_fades: bool,
}

#[derive(Clone)]
pub struct DebugConfig {
    /// Dump the code section once Steam DRM has decrypted it. The on-disk
    /// executable ships `.text` encrypted and cannot be disassembled.
    pub dump_text: bool,
    pub dump_path: String,
    /// Enables the hotkey-driven memory probe.
    pub probe: bool,
    /// Write a dump beside the game when its window stops responding.
    pub hang_dump: bool,
    /// Value the probe searches for. Re-read on every hotkey press.
    pub probe_value: i32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            log_level: Level::Info,
            log_path: "re0inv.log".to_string(),
            slots: DEFAULT_SLOTS,
            doors: DoorsConfig {
                skip: false,
                shorten_fades: true,
            },
            item_box: ItemBoxConfig {
                enabled: false,
                slots: DEFAULT_BOX_SLOTS,
            },
            debug: DebugConfig {
                dump_text: false,
                dump_path: "re0hd_text_dump.bin".to_string(),
                probe: false,
                hang_dump: true,
                probe_value: 0,
            },
        }
    }
}

impl Config {
    /// A missing or unreadable file is not an error: defaults apply.
    pub fn load(path: &Path) -> Config {
        let mut cfg = Config::default();

        let Ok(text) = std::fs::read_to_string(path) else {
            return cfg;
        };

        let entries = parse(&text);
        let get = |k: &str| entries.get(k).map(|s| s.as_str());

        if let Some(v) = get("mod") {
            cfg.enabled = parse_bool(v, cfg.enabled);
        }
        if let Some(v) = get("slots") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.slots = n;
            }
        }
        if let Some(v) = get("level") {
            cfg.log_level = Level::parse(v);
        }
        if let Some(v) = get("path") {
            if !v.is_empty() {
                cfg.log_path = v.to_string();
            }
        }
        if let Some(v) = get("itembox") {
            cfg.item_box.enabled = parse_bool(v, cfg.item_box.enabled);
        }
        if let Some(v) = get("boxslots") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.item_box.slots = n;
            }
        }
        if let Some(v) = get("skipdoors") {
            cfg.doors.skip = parse_bool(v, cfg.doors.skip);
        }
        if let Some(v) = get("shortenfades") {
            cfg.doors.shorten_fades = parse_bool(v, cfg.doors.shorten_fades);
        }
        if let Some(v) = get("dumptext") {
            cfg.debug.dump_text = parse_bool(v, cfg.debug.dump_text);
        }
        if let Some(v) = get("dumppath") {
            if !v.is_empty() {
                cfg.debug.dump_path = v.to_string();
            }
        }
        if let Some(v) = get("probe") {
            cfg.debug.probe = parse_bool(v, cfg.debug.probe);
        }
        if let Some(v) = get("hangdump") {
            cfg.debug.hang_dump = parse_bool(v, cfg.debug.hang_dump);
        }
        if let Some(v) = get("probevalue") {
            cfg.debug.probe_value = parse_i32(v).unwrap_or(cfg.debug.probe_value);
        }

        cfg
    }

    /// Clamps invalid values and reports what changed. Correcting silently
    /// would hide the gap between the requested and the applied setting.
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.slots < BAG_SIZE {
            warnings.push(format!(
                "Slots={} is below the stock {}; using {}.",
                self.slots, BAG_SIZE, BAG_SIZE
            ));
            self.slots = BAG_SIZE;
        }

        if !self.slots.is_multiple_of(2) {
            let fixed = self.slots + 1;
            warnings.push(format!(
                "Slots={} is odd and would break two-slot items; using {}.",
                self.slots, fixed
            ));
            self.slots = fixed;
        }

        if self.slots > MAX_SLOTS {
            warnings.push(format!(
                "Slots={} exceeds the supported maximum; using {}.",
                self.slots, MAX_SLOTS
            ));
            self.slots = MAX_SLOTS;
        }

        warnings
    }
}

fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        // Trailing comment: `Slots=8 ; note`
        let value = value.split(';').next().unwrap_or(value);
        map.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    map
}

fn parse_bool(v: &str, default: bool) -> bool {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

/// Accepts decimal and `0x` hex.
fn parse_i32(v: &str) -> Option<i32> {
    let v = v.trim();
    match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok().map(|n| n as i32),
        None => v.parse::<i32>().ok(),
    }
}
