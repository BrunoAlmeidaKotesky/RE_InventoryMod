//! Observing the inventory menu.
//!
//! Nothing here changes what the game does. It reports what the menu object
//! holds when the cursor moves, so the field that says *which character's panel
//! this is* can be identified from a running game instead of guessed at.
//!
//! # Why patch here and not at the refusal
//!
//! The interesting decision is the conditional jump that refuses to move past
//! the edge of the panel. Replacing that means reproducing a branch and
//! preserving the flags it reads, and it is the first mid-function patch in
//! this project.
//!
//! The instruction just before it, `mov eax, [edi+0x2BC]`, is six bytes, reads
//! the cursor, and touches no flags. Replacing that one exercises the whole
//! trampoline mechanism — page protection, a jump into our code, registers
//! preserved, control handed back mid-function — while being unable to change
//! any behaviour. Once it is proven, the refusal is a small step from here.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info};
use crate::debug::memory;
use crate::store::registry;

/// Cursor position within the panel, 0 to 5.
const OFFSET_CURSOR: usize = 0x2BC;
/// Read as a flag on the downward path.
const OFFSET_FLAG: usize = 0x2C6;

/// How much of the menu object to search for a bag pointer.
const SCAN_BYTES: usize = 0x600;

/// How far into an object the menu points at to keep searching.
///
/// The menu may hold the character or the inventory owner rather than the bag
/// itself, in which case the bag is one hop further on.
const NESTED_SCAN_BYTES: usize = 0x200;

/// How many distinct pointers to follow one level down. A bound keeps a menu
/// full of pointers from turning one keypress into a long pause.
const MAX_FOLLOWED: usize = 64;

/// Offsets of the two bags inside their parent object. A pointer to the parent
/// is as good as a pointer to the bag, so both are worth recognising.
const BAG_OFFSETS: [usize; 2] = [0x20, 0x60];

/// Observations logged before going quiet. Enough to cover moving around both
/// panels a few times.
const LOG_LIMIT: usize = 12;

static OBSERVATIONS: AtomicUsize = AtomicUsize::new(0);

/// Where each trampoline hands control back, filled in at install time.
///
/// The addresses are only known once the build is identified, so they cannot be
/// immediates in the assembly. A memory-indirect jump reads them without
/// needing a register, which matters because every register here belongs to the
/// game.
static UP_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static DOWN_CONTINUE: AtomicUsize = AtomicUsize::new(0);

pub fn set_up_continue(address: usize) {
    UP_CONTINUE.store(address, Ordering::Relaxed);
}

pub fn set_down_continue(address: usize) {
    DOWN_CONTINUE.store(address, Ordering::Relaxed);
}

/// Trampoline for the cursor read on the upward path.
///
/// # Safety
/// Reached only through the patch written over `mov eax, [edi+0x2BC]`, so `edi`
/// holds the menu object and the game expects `eax` to hold the cursor on the
/// way out.
#[unsafe(naked)]
pub unsafe extern "C" fn cursor_up_stub() {
    core::arch::naked_asm!(
        // Everything here belongs to the game. Flags first, then every register.
        "pushfd",
        "pushad",
        // Keep the old stack pointer somewhere pushad has already saved, then
        // align. Rust's generated code may use instructions that fault on a
        // stack that is not 16-byte aligned.
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], edi",
        "call {observe}",
        "mov esp, ebp",
        "popad",
        "popfd",
        // The instruction this replaced.
        "mov eax, [edi + 0x2BC]",
        "jmp dword ptr [{continue_at}]",
        observe = sym observe,
        continue_at = sym UP_CONTINUE,
    )
}

/// Trampoline for the cursor read on the downward path.
///
/// # Safety
/// Same contract as `cursor_up_stub`.
#[unsafe(naked)]
pub unsafe extern "C" fn cursor_down_stub() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], edi",
        "call {observe}",
        "mov esp, ebp",
        "popad",
        "popfd",
        "mov eax, [edi + 0x2BC]",
        "jmp dword ptr [{continue_at}]",
        observe = sym observe,
        continue_at = sym DOWN_CONTINUE,
    )
}

/// Reports the menu object's state. Reads only.
extern "C" fn observe(menu: usize) {
    let _ = std::panic::catch_unwind(|| {
        let seen = OBSERVATIONS.fetch_add(1, Ordering::Relaxed);

        if seen == LOG_LIMIT {
            log_debug!("menu: logged {LOG_LIMIT} observations, staying quiet from here.");
        }
        if seen >= LOG_LIMIT {
            return;
        }

        if menu == 0 || !menu.is_multiple_of(4) {
            log_info!("menu observation with an unusable pointer: 0x{menu:08X}");
            return;
        }

        // Safety: the game just used this pointer to read its own field, so the
        // object is mapped.
        let cursor = unsafe { *((menu + OFFSET_CURSOR) as *const i32) };
        let flag = unsafe { *((menu + OFFSET_FLAG) as *const u8) };

        log_info!("menu 0x{menu:08X}  cursor {cursor}  +0x2C6 {flag}");

        let bags = registry::known_bags();
        if bags.is_empty() {
            log_info!("  no bags known yet, so nothing to look for");
            return;
        }

        find_bag_pointer(menu, &bags);
    });
}

/// Searches the menu object for a pointer that leads to one of the bags.
///
/// The answer decides how the scroll hook will work. If the menu holds the bag
/// it is showing, the hook reads it from there and looks the store up by
/// address, and never has to reason about which character the panel belongs to.
fn find_bag_pointer(menu: usize, bags: &[usize]) {
    let Some(words) = read_words(menu, SCAN_BYTES) else {
        log_info!("  menu object is not readable");
        return;
    };

    let mut found = 0;
    let mut followed = 0;
    let mut seen: Vec<usize> = Vec::new();

    for (index, &value) in words.iter().enumerate() {
        let offset = index * 4;

        if let Some(description) = describe_target(value, bags) {
            log_info!("  menu+0x{offset:03X} -> {description}");
            found += 1;
            continue;
        }

        // Not a bag itself. If it looks like a pointer, follow it once: the
        // menu may hold the character, with the bag inside that.
        if !is_plausible_pointer(value) || seen.contains(&value) || followed >= MAX_FOLLOWED {
            continue;
        }

        seen.push(value);
        followed += 1;

        let Some(nested) = read_words(value, NESTED_SCAN_BYTES) else {
            continue;
        };

        for (inner_index, &target) in nested.iter().enumerate() {
            if let Some(description) = describe_target(target, bags) {
                let inner = inner_index * 4;
                log_info!("  menu+0x{offset:03X} -> 0x{value:08X} +0x{inner:03X} -> {description}");
                found += 1;
            }
        }
    }

    if found == 0 {
        log_info!("  no path from the menu to a known bag ({followed} pointers followed)");
    }
}

/// Reads a run of little-endian 32-bit words.
///
/// One call per object rather than one per word: this runs while the game is
/// mid-frame, and thousands of separate reads would show up as a stutter.
fn read_words(address: usize, bytes: usize) -> Option<Vec<usize>> {
    let mut buffer = vec![0u8; bytes];
    if !memory::read(address, &mut buffer) {
        return None;
    }

    Some(
        buffer
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_le_bytes(*chunk) as usize)
            .collect(),
    )
}

/// Names `value` if it is a bag, or the parent object holding one.
fn describe_target(value: usize, bags: &[usize]) -> Option<String> {
    for &bag in bags {
        if value == bag {
            return Some(format!("bag 0x{bag:08X}"));
        }

        for offset in BAG_OFFSETS {
            if bag.checked_sub(offset) == Some(value) {
                return Some(format!("parent of bag 0x{bag:08X}, at +0x{offset:02X}"));
            }
        }
    }

    None
}

/// Whether a value is worth dereferencing. Keeps the nested search away from
/// small integers and obvious rubbish; the read itself is checked anyway.
fn is_plausible_pointer(value: usize) -> bool {
    const USER_MIN: usize = 0x0001_0000;
    const USER_MAX: usize = 0x7FFF_0000;

    (USER_MIN..USER_MAX).contains(&value) && value.is_multiple_of(4)
}
