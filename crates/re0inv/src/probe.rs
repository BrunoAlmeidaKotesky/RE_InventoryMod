//! Runtime memory probe. Debug tool for locating game structures.
//!
//! Hotkey driven, all output goes to the log:
//!   F9  new scan for ProbeValue
//!   F10 refine previous hits to current ProbeValue
//!   F11 hexdump hits, also decoded as a candidate Bag
//!   F12 log the memory region map
//!
//! ProbeValue is re-read from the ini on every press, so it can be changed
//! without restarting the game.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use crate::config::Config;
use crate::log::{log_info, log_warn};

const MEM_COMMIT: u32 = 0x1000;
const PAGE_GUARD: u32 = 0x100;
const READABLE: u32 = 0x02 | 0x04 | 0x08 | 0x20 | 0x40 | 0x80;
const WRITABLE: u32 = 0x04 | 0x08 | 0x40 | 0x80;

const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;

/// Cap so a scan for a common value cannot exhaust memory.
const MAX_HITS: usize = 200_000;
/// Hits written to the log per press.
const MAX_LOGGED: usize = 40;

static HITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Copies via ReadProcessMemory instead of dereferencing: a freed or protected
/// region returns an error rather than crashing the game.
fn read(addr: usize, out: &mut [u8]) -> bool {
    let mut got: usize = 0;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            addr as *const _,
            out.as_mut_ptr() as *mut _,
            out.len(),
            &mut got,
        )
    };
    ok != 0 && got == out.len()
}

fn read_i32(addr: usize) -> Option<i32> {
    let mut buf = [0u8; 4];
    if read(addr, &mut buf) {
        Some(i32::from_le_bytes(buf))
    } else {
        None
    }
}

struct Region {
    base: usize,
    size: usize,
    protect: u32,
    writable: bool,
}

fn regions() -> Vec<Region> {
    let mut out = Vec::new();
    let mut addr: usize = 0x10000;
    let limit: usize = 0x7FFF_0000;

    while addr < limit {
        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let n = unsafe {
            VirtualQuery(
                addr as *const _,
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if n == 0 {
            break;
        }

        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        let usable =
            mbi.State == MEM_COMMIT && mbi.Protect & PAGE_GUARD == 0 && mbi.Protect & READABLE != 0;

        if usable && size > 0 {
            out.push(Region {
                base,
                size,
                protect: mbi.Protect,
                writable: mbi.Protect & WRITABLE != 0,
            });
        }

        let next = base.saturating_add(size);
        if next <= addr {
            break;
        }
        addr = next;
    }

    out
}

/// Scans readable regions for a 4-byte aligned i32.
fn scan(value: i32) -> Vec<usize> {
    let needle = value.to_le_bytes();
    let mut hits = Vec::new();
    let mut buf = vec![0u8; 1 << 20];

    for r in regions() {
        let mut off = 0usize;
        while off < r.size {
            let chunk = buf.len().min(r.size - off);
            let slice = &mut buf[..chunk];
            if !read(r.base + off, slice) {
                off += chunk;
                continue;
            }

            let mut i = 0usize;
            while i + 4 <= chunk {
                if slice[i..i + 4] == needle {
                    hits.push(r.base + off + i);
                    if hits.len() >= MAX_HITS {
                        return hits;
                    }
                }
                i += 4;
            }

            off += chunk;
        }
    }

    hits
}

fn refine(previous: &[usize], value: i32) -> Vec<usize> {
    previous
        .iter()
        .copied()
        .filter(|&a| read_i32(a) == Some(value))
        .collect()
}

fn hexdump(addr: usize, len: usize) {
    let mut buf = vec![0u8; len];
    if !read(addr, &mut buf) {
        log_warn!("0x{:08X}: unreadable", addr);
        return;
    }

    let rows = (len + 15) / 16;
    for row in 0..rows {
        let start = row * 16;
        let end = (start + 16).min(len);
        let hex: Vec<String> = buf[start..end].iter().map(|b| format!("{:02X}", b)).collect();
        let ascii: String = buf[start..end]
            .iter()
            .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
            .collect();
        log_info!("  0x{:08X}  {:<47} |{}|", addr + start, hex.join(" "), ascii);
    }
}

/// Decodes 64 bytes as the assumed Bag layout, to sanity-check a candidate.
/// Reference layout: i32 unknown, 6 items, personal item, equipped index.
fn decode_bag(addr: usize) {
    let mut buf = [0u8; 64];
    if !read(addr, &mut buf) {
        return;
    }

    let at = |o: usize| i32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);

    log_info!("  as Bag @ 0x{:08X}:", addr);
    log_info!("    +0x00 unknown        = {}", at(0));
    for i in 0..6 {
        let o = 0x04 + i * 8;
        log_info!(
            "    +0x{:02X} item[{}]       = id {:>5}  count {:>5}",
            o,
            i,
            at(o),
            at(o + 4)
        );
    }
    log_info!("    +0x34 personal item  = id {:>5}  count {:>5}", at(0x34), at(0x38));
    log_info!("    +0x3C equipped index = {}", at(0x3C));
}

fn log_regions() {
    let list = regions();
    let total: usize = list.iter().map(|r| r.size).sum();
    log_info!(
        "Memory map: {} readable regions, {} KB committed.",
        list.len(),
        total / 1024
    );
    for r in &list {
        log_info!(
            "  0x{:08X} - 0x{:08X}  {:>8} KB  protect 0x{:03X}{}",
            r.base,
            r.base + r.size,
            r.size / 1024,
            r.protect,
            if r.writable { " W" } else { "" }
        );
    }
}

fn do_scan(ini: &PathBuf) {
    let value = Config::load(ini).probe_value;
    log_info!("Scan for {} (0x{:08X})...", value, value);

    let hits = scan(value);
    log_info!("{} hits.", hits.len());
    for a in hits.iter().take(MAX_LOGGED) {
        log_info!("  0x{:08X}", a);
    }
    if hits.len() > MAX_LOGGED {
        log_info!("  ... {} more not listed", hits.len() - MAX_LOGGED);
    }

    *HITS.lock().unwrap() = hits;
}

fn do_refine(ini: &PathBuf) {
    let value = Config::load(ini).probe_value;
    let mut guard = HITS.lock().unwrap();

    if guard.is_empty() {
        log_warn!("No previous scan. Press F9 first.");
        return;
    }

    let before = guard.len();
    *guard = refine(&guard, value);
    log_info!("Refine to {}: {} -> {} hits.", value, before, guard.len());
    for a in guard.iter().take(MAX_LOGGED) {
        log_info!("  0x{:08X}", a);
    }
}

fn do_inspect() {
    let guard = HITS.lock().unwrap();
    if guard.is_empty() {
        log_warn!("Nothing to inspect.");
        return;
    }

    log_info!("Inspecting {} of {} hits.", guard.len().min(8), guard.len());
    for &a in guard.iter().take(8) {
        // Step back to where a Bag would start if this address is item[0].id.
        let bag = a.saturating_sub(0x04);
        log_info!("hit 0x{:08X}:", a);
        hexdump(bag, 64);
        decode_bag(bag);
    }
}

/// Polls hotkeys forever. Runs on its own thread.
pub fn run(ini: PathBuf) {
    log_info!("Memory probe active: F9 scan, F10 refine, F11 inspect, F12 memory map.");

    let keys = [VK_F9, VK_F10, VK_F11, VK_F12];
    let mut down = [false; 4];

    loop {
        for (i, &vk) in keys.iter().enumerate() {
            let pressed = unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;

            // Edge trigger: act once per press, not once per poll.
            if pressed && !down[i] {
                match vk {
                    VK_F9 => do_scan(&ini),
                    VK_F10 => do_refine(&ini),
                    VK_F11 => do_inspect(),
                    VK_F12 => log_regions(),
                    _ => {}
                }
            }
            down[i] = pressed;
        }

        std::thread::sleep(Duration::from_millis(60));
    }
}
