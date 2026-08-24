//! Pointing the game at this mod's own copy of a message archive.
//!
//! The game builds the path of a message archive by formatting a three-letter
//! language name into `nativePC\arc\message\msg_%s.arc`. At the moment the
//! format string is pushed, the language name is already on the stack.
//!
//! So the whole redirection is: replace that push with a jump, hand the
//! language name to a function that may answer with a different one, put the
//! answer back on the stack, push the format string, and carry on. The game
//! then loads `msg_eng_inv.arc` believing it asked for `msg_eng`.
//!
//! Nothing is written over the player's own archives. If the mod's copy is not
//! there — never built, or deleted — the language name comes back unchanged and
//! the game loads what it always did.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::hook::patch::Patch;

/// The language names this mod ships an archive for, each already terminated
/// so the address of the first byte can be handed straight to the game.
///
/// Static rather than built on demand: the game keeps the pointer, and anything
/// allocated here would have to outlive a call it knows nothing about.
const REDIRECTED: [&[u8]; 8] = [
    b"chS_inv\0",
    b"chT_inv\0",
    b"eng_inv\0",
    b"fre_inv\0",
    b"ger_inv\0",
    b"ita_inv\0",
    b"jpn_inv\0",
    b"spa_inv\0",
];

/// Length of a language name, without the terminator.
const NAME: usize = 3;

/// The format string the replaced instruction pushed.
static FORMAT: AtomicUsize = AtomicUsize::new(0);

/// Where each of the three trampolines rejoins the code it came from.
///
/// One static apiece rather than an array: `sym` in inline assembly takes a
/// path to an item, and an indexed element is not one.
static FIRST_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static SECOND_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static THIRD_CONTINUE: AtomicUsize = AtomicUsize::new(0);

/// `push imm32`, the instruction being replaced at all three sites.
const PUSH_IMM32: u8 = 0x68;
const NOP: u8 = 0x90;

pub unsafe fn install(addresses: &Addresses, patches: &mut Vec<Patch>) {
    FORMAT.store(addresses.message_format, Ordering::Relaxed);

    let mut expected = [NOP; 5];
    expected[0] = PUSH_IMM32;
    expected[1..].copy_from_slice(&(addresses.message_format as u32).to_le_bytes());

    let stubs: [(unsafe extern "C" fn(), &AtomicUsize); 3] = [
        (first_stub, &FIRST_CONTINUE),
        (second_stub, &SECOND_CONTINUE),
        (third_stub, &THIRD_CONTINUE),
    ];

    for (index, &site) in addresses.message_language.iter().enumerate() {
        let (stub, continue_at) = stubs[index];
        continue_at.store(site + expected.len(), Ordering::Relaxed);

        let Some(jump) = crate::hook::detour::jump_bytes(site, stub as usize) else {
            log_warn!("Message redirect: could not build the jump for site {index}.");
            continue;
        };

        match Patch::write_expecting(site, &expected, &jump) {
            Some(patch) => patches.push(patch),
            None => log_warn!("Message redirect: site {index} is not the expected instruction."),
        }
    }
}

/// The three sites differ only in where they hand control back.
macro_rules! redirect_stub {
    ($name:ident, $continue_at:ident) => {
        /// # Safety
        /// Reached only through the jump written over the push, so the language
        /// name is on the stack where the game left it.
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                // The name is at [esp+4] once the call pushes a return address,
                // which is where a C function looks for its first argument.
                "call {choose}",
                // Answer in its place, so the format below sees it.
                "mov [esp], eax",
                "push dword ptr [{format}]",
                "jmp dword ptr [{continue_at}]",
                choose = sym choose_language,
                format = sym FORMAT,
                continue_at = sym $continue_at,
            )
        }
    };
}

redirect_stub!(first_stub, FIRST_CONTINUE);
redirect_stub!(second_stub, SECOND_CONTINUE);
redirect_stub!(third_stub, THIRD_CONTINUE);

/// Answers with this mod's language name when its archive is there.
extern "C" fn choose_language(language: *const u8) -> *const u8 {
    let result = std::panic::catch_unwind(|| {
        if language.is_null() {
            return language;
        }

        // Safety: the game is about to format this as a string, so three bytes
        // of it are readable by the time it reaches here.
        let asked = unsafe { std::slice::from_raw_parts(language, NAME) };

        let Some(name) = REDIRECTED.iter().find(|name| name[..NAME] == *asked) else {
            return language;
        };

        if !archive_exists(name) {
            return language;
        }

        name.as_ptr()
    });

    result.unwrap_or(language)
}

/// Whether the archive for a redirected name is actually on disk.
///
/// Checked every time rather than once. The answer only changes if someone
/// deletes the file while the game is running, and answering with a name whose
/// file is gone would leave the game with no messages at all.
fn archive_exists(name: &[u8]) -> bool {
    let Ok(name) = std::str::from_utf8(&name[..name.len() - 1]) else {
        return false;
    };

    let path = crate::game::module::game_directory()
        .join("nativePC")
        .join("arc")
        .join("message")
        .join(format!("msg_{name}.arc"));

    let there = path.exists();

    if !there {
        log_missing(&path);
    }

    there
}

/// Says once that an archive is missing, rather than on every load.
fn log_missing(path: &std::path::Path) {
    use std::sync::atomic::AtomicBool;
    static SAID: AtomicBool = AtomicBool::new(false);

    if !SAID.swap(true, Ordering::Relaxed) {
        log_info!(
            "No modded message archive at {}; the typewriter keeps its own prompt.",
            path.display()
        );
    }
}
