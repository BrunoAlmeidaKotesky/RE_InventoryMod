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

/// Cursor position within the panel, 0 to 5.
const OFFSET_CURSOR: usize = 0x2BC;
/// Read as a flag on the downward path. Candidate for the panel selector.
const OFFSET_FLAG: usize = 0x2C6;

/// Region of the menu object to dump, chosen to bracket both fields above.
const DUMP_START: usize = 0x2A0;
const DUMP_LENGTH: usize = 0x40;

/// Observations logged before going quiet. Enough to cover moving around both
/// panels a few times.
const LOG_LIMIT: usize = 24;

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
        // object is mapped. The dump stays inside the same object.
        let cursor = unsafe { *((menu + OFFSET_CURSOR) as *const i32) };
        let flag = unsafe { *((menu + OFFSET_FLAG) as *const u8) };

        log_info!(
            "menu 0x{:08X}  cursor {}  +0x2C6 {}",
            menu,
            cursor,
            flag
        );

        for row in 0..(DUMP_LENGTH / 16) {
            let base = menu + DUMP_START + row * 16;
            let words: Vec<String> = (0..4)
                .map(|i| {
                    let value = unsafe { *((base + i * 4) as *const u32) };
                    format!("{value:08X}")
                })
                .collect();

            log_info!("  +0x{:03X}  {}", DUMP_START + row * 16, words.join(" "));
        }
    });
}
