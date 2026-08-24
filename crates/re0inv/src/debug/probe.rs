//! Memory probe.
//!
//! The workflow it supports is the standard narrowing search: note a value you
//! can see in game (an item id, an ammo count), scan for it, change it in game,
//! scan again for the new value, repeat until one address survives.
//!
//! The keys that drive it are bound in `core::input`; these are the commands
//! behind them. `ProbeValue` is re-read from the ini on every call, so it can be
//! changed while the game runs.

use std::path::Path;
use std::sync::Mutex;

use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::debug::memory;
use crate::game::inventory::{Bag, BAG_BYTES};

/// Cap so a scan for a common value cannot exhaust memory.
const MAX_HITS: usize = 200_000;
/// Addresses written to the log per press.
const MAX_LOGGED: usize = 40;
/// Candidates inspected in detail by F11.
const MAX_INSPECTED: usize = 8;

static HITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());


fn probe_value(ini: &Path) -> i32 {
    Config::load(ini).debug.probe_value
}

pub fn scan(ini: &Path) {
    let value = probe_value(ini);
    log_info!("Scanning for {} (0x{:08X})...", value, value);

    let hits = memory::scan_i32(value, MAX_HITS);
    report(&hits);
    *HITS.lock().unwrap() = hits;
}

pub fn narrow(ini: &Path) {
    let value = probe_value(ini);
    let mut hits = HITS.lock().unwrap();

    if hits.is_empty() {
        log_warn!("No previous scan to narrow. Press F9 first.");
        return;
    }

    let before = hits.len();
    *hits = memory::narrow(&hits, value);
    log_info!("Narrowed to {}: {} -> {} hits.", value, before, hits.len());
    report(&hits);
}

fn report(hits: &[usize]) {
    log_info!("{} hits.", hits.len());
    for addr in hits.iter().take(MAX_LOGGED) {
        log_info!("  0x{:08X}", addr);
    }
    if hits.len() > MAX_LOGGED {
        log_info!("  ... {} more not listed", hits.len() - MAX_LOGGED);
    }
}

/// Dumps each surviving candidate and decodes it as a Bag.
///
/// A hit on an item id would be `items[0].id`, which sits at +0x04 in the
/// struct, so the candidate Bag starts one field earlier.
pub fn inspect() {
    let hits = HITS.lock().unwrap();

    if hits.is_empty() {
        log_warn!("Nothing to inspect.");
        return;
    }

    log_info!(
        "Inspecting {} of {} hits.",
        hits.len().min(MAX_INSPECTED),
        hits.len()
    );

    for &addr in hits.iter().take(MAX_INSPECTED) {
        let bag_start = addr.saturating_sub(0x04);
        log_info!("hit 0x{:08X}:", addr);
        hexdump(bag_start, BAG_BYTES);
        decode_bag(bag_start);
    }
}

fn hexdump(addr: usize, len: usize) {
    let mut buf = vec![0u8; len];
    if !memory::read(addr, &mut buf) {
        log_warn!("  0x{:08X}: unreadable", addr);
        return;
    }

    for start in (0..len).step_by(16) {
        let end = (start + 16).min(len);
        let row = &buf[start..end];

        let hex: Vec<String> = row.iter().map(|b| format!("{:02X}", b)).collect();
        let ascii: String = row
            .iter()
            .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
            .collect();

        log_info!("  0x{:08X}  {:<47} |{}|", addr + start, hex.join(" "), ascii);
    }
}

fn decode_bag(addr: usize) {
    let Some(buf) = memory::read_array::<BAG_BYTES>(addr) else {
        return;
    };

    let bag = Bag::from_bytes(&buf);

    log_info!(
        "  as Bag @ 0x{:08X}{}:",
        addr,
        if bag.looks_plausible() { "  [PLAUSIBLE]" } else { "" }
    );
    log_info!("    +0x00 unknown        = {}", bag.unknown00);
    for (i, item) in bag.items.iter().enumerate() {
        log_info!(
            "    +0x{:02X} item[{}]       = id {:>5}  count {:>5}",
            0x04 + i * 8,
            i,
            item.id,
            item.count
        );
    }
    log_info!(
        "    +0x34 personal item  = id {:>5}  count {:>5}",
        bag.personal_item.id,
        bag.personal_item.count
    );
    log_info!("    +0x3C equipped index = {}", bag.equipped_index);
}

pub fn log_regions() {
    let regions = memory::regions();
    let committed: usize = regions.iter().map(|r| r.size).sum();

    log_info!(
        "Memory map: {} readable regions, {} KB committed.",
        regions.len(),
        committed / 1024
    );

    for r in &regions {
        log_info!(
            "  0x{:08X} - 0x{:08X}  {:>8} KB  protect 0x{:03X}{}",
            r.base,
            r.end(),
            r.size / 1024,
            r.protect,
            if r.writable { " W" } else { "" }
        );
    }
}
