//! `re0inv.ini` reader. Flat `key=value`, sections ignored, `;` and `#` comment.

use std::collections::HashMap;
use std::path::Path;

use crate::log::Level;

pub struct Config {
    pub enabled: bool,
    pub log_level: Level,
    pub log_path: String,
    /// Inventory slots per character. Must be even: the game lays the inventory
    /// out in rows of 2 and two-slot items break on an odd count.
    pub slots: usize,
    /// Dump the code section once the Steam DRM stub has decrypted it.
    /// The on-disk executable has `.text` encrypted and cannot be disassembled.
    pub dump_text: bool,
    pub dump_path: String,
    /// Enables the hotkey-driven memory probe.
    pub probe: bool,
    /// Value the probe searches for. Re-read on every hotkey press.
    pub probe_value: i32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            log_level: Level::Info,
            log_path: "re0inv.log".to_string(),
            slots: 8,
            dump_text: false,
            dump_path: "re0hd_text_dump.bin".to_string(),
            probe: false,
            probe_value: 0,
        }
    }
}

impl Config {
    /// Missing or unreadable file is not an error: defaults apply.
    pub fn load(path: &Path) -> Config {
        let mut cfg = Config::default();

        let Ok(text) = std::fs::read_to_string(path) else {
            return cfg;
        };

        let mut map: HashMap<String, String> = HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with(';')
                || line.starts_with('#')
                || line.starts_with('[')
            {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.split(';').next().unwrap_or(v);
            map.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }

        let get = |k: &str| map.get(k).map(|s| s.as_str());

        if let Some(v) = get("mod") {
            cfg.enabled = parse_bool(v, cfg.enabled);
        }
        if let Some(v) = get("level") {
            cfg.log_level = Level::from_str(v);
        }
        if let Some(v) = get("path") {
            if !v.is_empty() {
                cfg.log_path = v.to_string();
            }
        }
        if let Some(v) = get("slots") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.slots = n;
            }
        }
        if let Some(v) = get("dumptext") {
            cfg.dump_text = parse_bool(v, cfg.dump_text);
        }
        if let Some(v) = get("dumppath") {
            if !v.is_empty() {
                cfg.dump_path = v.to_string();
            }
        }
        if let Some(v) = get("probe") {
            cfg.probe = parse_bool(v, cfg.probe);
        }
        if let Some(v) = get("probevalue") {
            cfg.probe_value = parse_i32(v).unwrap_or(cfg.probe_value);
        }

        cfg
    }

    /// Clamps invalid values and returns what was changed. Silent correction
    /// would hide the difference between the requested and applied setting.
    pub fn sanitize(&mut self) -> Vec<String> {
        const MAX_SLOTS: usize = 32;
        let mut warnings = Vec::new();

        if self.slots < 6 {
            warnings.push(format!("Slots={} is below the stock 6; using 6.", self.slots));
            self.slots = 6;
        }

        if self.slots % 2 != 0 {
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
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok().map(|n| n as i32)
    } else {
        v.parse::<i32>().ok()
    }
}
