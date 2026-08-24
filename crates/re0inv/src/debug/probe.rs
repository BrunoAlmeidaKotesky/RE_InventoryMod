//! Hotkey-driven memory probe.
//!
//! The workflow it supports is the standard narrowing search: note a value you
//! can see in game (an item id, an ammo count), scan for it, change it in game,
//! scan again for the new value, repeat until one address survives.
//!
//!   F8  remove every installed hook, restoring the game's own code
//!   F9  scan for ProbeValue
//!   F10 narrow the previous hits to the current ProbeValue
//!   F11 dump the surviving hits, decoded as candidate Bags
//!   F12 log the memory region map
//!
//! ProbeValue is re-read from the ini on every press, so it can be changed
//! while the game runs.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::win32::{GetAsyncKeyState, KEY_PRESSED};

use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::debug::memory;
use crate::game::inventory::{Bag, BAG_BYTES, BAG_SIZE};
use crate::store::registry;

/// Page Up and Page Down scroll the inventory window.
const VK_PAGE_UP: i32 = 0x21;
const VK_PAGE_DOWN: i32 = 0x22;

const VK_F8: i32 = 0x77;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;

const POLL_INTERVAL: Duration = Duration::from_millis(60);

/// Cap so a scan for a common value cannot exhaust memory.
const MAX_HITS: usize = 200_000;
/// Addresses written to the log per press.
const MAX_LOGGED: usize = 40;
/// Candidates inspected in detail by F11.
const MAX_INSPECTED: usize = 8;

static HITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Polls the hotkeys forever. Runs on its own thread.
pub fn run(ini: PathBuf, debug_keys: bool) {
    log_info!("Page Up and Page Down scroll the inventory.");
    if debug_keys {
        log_info!("Debug keys: F8 remove hooks, F9 scan, F10 narrow, F11 inspect, F12 memory map.");
    }

    let keys = [
        VK_PAGE_UP,
        VK_PAGE_DOWN,
        VK_F8,
        VK_F9,
        VK_F10,
        VK_F11,
        VK_F12,
    ];
    let mut was_down = [false; 7];

    loop {
        for (i, &key) in keys.iter().enumerate() {
            let down = unsafe { GetAsyncKeyState(key) } as u16 & KEY_PRESSED != 0;

            // Edge trigger: act once per press, not once per poll.
            if down && !was_down[i] {
                dispatch(key, &ini, debug_keys);
            }
            was_down[i] = down;
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

fn dispatch(key: i32, ini: &Path, debug_keys: bool) {
    // Scrolling is a feature, not a diagnostic, so it works regardless of
    // whether the debug keys are switched on.
    match key {
        VK_PAGE_UP => scroll(-1),
        VK_PAGE_DOWN => scroll(1),
        _ if !debug_keys => {}
        VK_F8 => unsafe { crate::hook::remove_all_installed() },
        VK_F9 => scan(ini),
        VK_F10 => narrow(ini),
        VK_F11 => inspect(),
        VK_F12 => log_regions(),
        _ => {}
    }
}

fn probe_value(ini: &Path) -> i32 {
    Config::load(ini).debug.probe_value
}

fn scan(ini: &Path) {
    let value = probe_value(ini);
    log_info!("Scanning for {} (0x{:08X})...", value, value);

    let hits = memory::scan_i32(value, MAX_HITS);
    report(&hits);
    *HITS.lock().unwrap() = hits;
}

fn narrow(ini: &Path) {
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
fn inspect() {
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

fn log_regions() {
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

/// Moves every inventory window by one row and reports where they landed.
fn scroll(rows: i32) {
    let moved = registry::scroll_all(rows);

    if moved == 0 {
        log_info!("Nothing to scroll.");
        return;
    }

    for (bag, position, capacity) in registry::positions() {
        log_info!(
            "Bag 0x{:08X}: showing slots {}-{} of {}.",
            bag,
            position + 1,
            position + BAG_SIZE,
            capacity
        );
    }
}
