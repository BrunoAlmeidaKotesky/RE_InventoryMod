//! Finding the field that holds the inventory selection.
//!
//! The field currently read as the cursor, `menu+0x2BC`, was wrong: a whole
//! session of moving around the panel logged `Some(0)` every single time. Since
//! scrolling at the bottom row depends on knowing where the selection is, the
//! field has to be found rather than guessed at again.
//!
//! The method is a differential scan. The menu object is snapshotted every
//! poll, and any word that changed to a different value in the range a
//! six-slot selection can hold is counted. A field that keeps doing that while
//! the player moves around the panel is a candidate; nothing else in the object
//! behaves that way for long.
//!
//! This is diagnostic only and runs behind the probe switch.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::logging::log_info;
use crate::debug::memory;
use crate::hook::panel;

/// How much of the menu object to watch. The field wrongly taken for the cursor
/// was at `0x2BC`, so the object is at least that big; this covers a good
/// margin past it.
const SCAN_BYTES: usize = 0x800;

/// Highest value a selection over six slots can hold.
const SELECTION_MAX: i32 = 5;

/// Changes a field must make before it is worth reporting.
///
/// One change proves nothing — plenty of counters pass through small numbers.
/// Several, all landing inside the selection's range, is a different matter.
const REPORT_AFTER: u32 = 5;

/// Fields reported before going quiet, so a noisy object cannot fill the log.
const REPORT_LIMIT: usize = 12;

struct Watch {
    menu: usize,
    previous: Vec<u8>,
    hits: HashMap<usize, u32>,
    reported: usize,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

/// Compares the menu object against the last look at it.
///
/// Call this on every input poll. It does nothing until the inventory has been
/// drawn at least once, and nothing at all when there is no menu.
pub fn sample() {
    let Some(menu) = panel::menu() else { return };

    let mut current = vec![0u8; SCAN_BYTES];
    if !memory::read(menu, &mut current) {
        return;
    }

    let Ok(mut watch) = WATCH.lock() else { return };

    // A different menu object means a different inventory screen, and counts
    // gathered against the old one say nothing about this one.
    let restart = match watch.as_ref() {
        Some(watch) => watch.menu != menu,
        None => true,
    };

    if restart {
        *watch = Some(Watch {
            menu,
            previous: current,
            hits: HashMap::new(),
            reported: 0,
        });
        return;
    }

    let Some(watch) = watch.as_mut() else { return };

    for offset in (0..SCAN_BYTES).step_by(4) {
        let was = word(&watch.previous, offset);
        let now = word(&current, offset);

        if was == now {
            continue;
        }

        // Both ends inside the range rules out counters that merely pass
        // through it on their way somewhere else.
        if !in_range(was) || !in_range(now) {
            continue;
        }

        let hits = watch.hits.entry(offset).or_insert(0);
        *hits += 1;

        if *hits == REPORT_AFTER && watch.reported < REPORT_LIMIT {
            watch.reported += 1;
            log_info!(
                "Selection candidate: menu+0x{offset:03X} keeps moving inside 0-{SELECTION_MAX} \
                 (now {was} -> {now})."
            );
        }
    }

    watch.previous = current;
}

/// Logs every candidate found so far, best first.
pub fn report() {
    let Ok(watch) = WATCH.lock() else { return };
    let Some(watch) = watch.as_ref() else {
        log_info!("Selection scan: the inventory has not been drawn yet.");
        return;
    };

    let mut ranked: Vec<_> = watch.hits.iter().map(|(&at, &hits)| (hits, at)).collect();
    ranked.sort_unstable_by(|a, b| b.cmp(a));

    if ranked.is_empty() {
        log_info!("Selection scan: nothing in menu 0x{:08X} moved yet.", watch.menu);
        return;
    }

    log_info!("Selection scan against menu 0x{:08X}:", watch.menu);
    for (hits, at) in ranked.iter().take(REPORT_LIMIT) {
        let value = memory::read_i32(watch.menu + at);
        log_info!("  menu+0x{at:03X}: {hits} move(s), currently {value:?}");
    }
}

fn word(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn in_range(value: i32) -> bool {
    (0..=SELECTION_MAX).contains(&value)
}
