//! Putting the item box on the typewriter prompt.
//!
//! # How the prompt works
//!
//! The typewriter is a script opcode, not hand-written C++. `0x0057A9D0`
//! handles it across several frames: `[this+0x38]` says whether this is the
//! first frame or a resumption, and `[this+0x10]` is a sub-state held as a
//! float that the resumed frame dispatches on.
//!
//! The first frame asks whether the player has an ink ribbon and shows one of
//! two messages:
//!
//! ```text
//! 12  ...Record data?<SEL Yes_No>          sub-state 2.0
//! 13  ...If you have an ink ribbon...      sub-state 0.0
//! ```
//!
//! Only sub-state `2.0` reaches the code that reads which choice was made.
//! That is exactly why the second prompt asks nothing: it is a statement, and
//! the game never comes back for an answer.
//!
//! # The four changes
//!
//! 1. Both message branches record whether there was a ribbon, and both now
//!    leave sub-state `2.0`. The second prompt starts asking.
//! 2. `cmp [esp+0x60], 1` becomes a call. The flags it left behind are produced
//!    here instead, so the branch after it still works.
//! 3. The call that opens the save screen becomes a call to a trampoline which,
//!    when the box was chosen, decrements the phase on the stack. Phase 6 is
//!    saving and phase 5 is the inventory, so one `dec` is the whole
//!    difference.
//! 4. The three sites that build a message archive's path get the language name
//!    swapped for one naming this mod's own copy.
//!
//! # Which choice means what
//!
//! With a ribbon the prompt is `Yes_No_Use`: 1 saves, 2 declines, 3 opens the
//! box. Without one it is `Yes_No` on a question about the box alone, so 1
//! opens it. Whether there was a ribbon is recorded on the first frame rather
//! than asked again later, because by then the answer could have changed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::hook::patch::Patch;

/// The choice that declines, in both prompts.
const CHOICE_NO: i32 = 2;
/// The choice that opens the box when a ribbon made a third one possible.
const CHOICE_USE: i32 = 3;
/// The choice that opens it when there was nothing else to ask.
const CHOICE_YES: i32 = 1;

/// `mov dword ptr [edi+0x10], 2.0f` - the sub-state that reaches the choice.
const SET_ASKING: [u8; 7] = [0xC7, 0x47, 0x10, 0x00, 0x00, 0x00, 0x40];
/// The same instruction with `0.0f`, which is what the no-ribbon branch had.
const SET_SILENT: [u8; 7] = [0xC7, 0x47, 0x10, 0x00, 0x00, 0x00, 0x00];
/// `cmp dword ptr [esp+0x60], 1`.
const COMPARE_CHOICE: [u8; 5] = [0x83, 0x7C, 0x24, 0x60, 0x01];

/// Whether the player had an ink ribbon when the prompt was shown.
static HAD_RIBBON: AtomicBool = AtomicBool::new(false);
/// Set when the answer was "open the box", read one call later.
static OPEN_WANTED: AtomicBool = AtomicBool::new(false);

/// Where each trampoline rejoins the code it was reached from.
static HAS_RIBBON_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static NO_RIBBON_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static SET_ROOM_PHASE: AtomicUsize = AtomicUsize::new(0);

/// Everything needed to open the inventory screen, filled in at install time.
static PREPARE_INVENTORY: AtomicUsize = AtomicUsize::new(0);
static INVENTORY_HOLDER: AtomicUsize = AtomicUsize::new(0);

pub struct Typewriter {
    patches: Vec<Patch>,
}

impl Typewriter {
    /// # Safety
    /// The addresses must belong to the build actually running, and the code
    /// section must be decrypted.
    pub unsafe fn install(addresses: &Addresses) -> Typewriter {
        let mut patches = Vec::new();

        HAS_RIBBON_CONTINUE.store(addresses.typewriter_has_ribbon + SET_ASKING.len(), Ordering::Relaxed);
        NO_RIBBON_CONTINUE.store(addresses.typewriter_no_ribbon + SET_SILENT.len(), Ordering::Relaxed);
        SET_ROOM_PHASE.store(addresses.set_room_phase, Ordering::Relaxed);
        PREPARE_INVENTORY.store(addresses.prepare_inventory, Ordering::Relaxed);
        INVENTORY_HOLDER.store(addresses.inventory_holder, Ordering::Relaxed);

        crate::hook::detour::jump_over(
            &mut patches,
            "prompt with a ribbon",
            addresses.typewriter_has_ribbon,
            &SET_ASKING,
            had_ribbon_stub as unsafe extern "C" fn() as usize,
        );

        crate::hook::detour::jump_over(
            &mut patches,
            "prompt without a ribbon",
            addresses.typewriter_no_ribbon,
            &SET_SILENT,
            no_ribbon_stub as unsafe extern "C" fn() as usize,
        );

        call_instead(
            &mut patches,
            "the choice made",
            addresses.typewriter_choice,
            &COMPARE_CHOICE,
            choice_stub as unsafe extern "C" fn() as usize,
        );

        // Whatever this call points at is where the trampoline hands control
        // back, so it is read rather than assumed.
        let open_screen = crate::hook::detour::call_target(addresses.typewriter_open_screen);
        if open_screen != Some(addresses.set_room_phase) {
            log_warn!("The typewriter does not open a screen where expected; the box will not open.");
        } else {
            call_instead(
                &mut patches,
                "opening the screen",
                addresses.typewriter_open_screen,
                &crate::hook::detour::call_bytes(
                    addresses.typewriter_open_screen,
                    addresses.set_room_phase,
                ),
                open_screen_stub as unsafe extern "C" fn() as usize,
            );
        }

        message::install(addresses, &mut patches);

        log_info!("Item box on the typewriter: {} patch(es) applied.", patches.len());

        Typewriter { patches }
    }

    /// # Safety
    /// The game module must still be mapped.
    pub unsafe fn remove(&mut self) {
        for patch in self.patches.iter().rev() {
            patch.revert();
        }
        self.patches.clear();
    }
}

/// Replaces a five-byte instruction with a call to our own code.
unsafe fn call_instead(
    patches: &mut Vec<Patch>,
    what: &str,
    at: usize,
    expected: &[u8],
    handler: usize,
) {
    let bytes = crate::hook::detour::call_bytes(at, handler);
    push(patches, what, Patch::write_expecting(at, expected, &bytes));
}

fn push(patches: &mut Vec<Patch>, what: &str, patch: Option<Patch>) {
    match patch {
        Some(patch) => patches.push(patch),
        None => log_warn!("Typewriter: could not patch {what}."),
    }
}

// --- The trampolines ---

/// Records that there was an ink ribbon, then does what it replaced.
///
/// # Safety
/// Reached only through the jump written over that instruction.
#[unsafe(naked)]
unsafe extern "C" fn had_ribbon_stub() {
    core::arch::naked_asm!(
        "pushad",
        "push 1",
        "call {note}",
        "add esp, 4",
        "popad",
        // The instruction this replaced, unchanged.
        "mov dword ptr [edi + 0x10], 0x40000000",
        "jmp dword ptr [{continue_at}]",
        note = sym note_ribbon,
        continue_at = sym HAS_RIBBON_CONTINUE,
    )
}

/// The same without a ribbon, except that the sub-state is changed.
///
/// The original left `0.0f`, which never reaches the code that reads a choice.
/// Writing the asking sub-state instead is what makes the second prompt a
/// question rather than a statement.
///
/// # Safety
/// Reached only through the jump written over that instruction.
#[unsafe(naked)]
unsafe extern "C" fn no_ribbon_stub() {
    core::arch::naked_asm!(
        "pushad",
        "push 0",
        "call {note}",
        "add esp, 4",
        "popad",
        "mov dword ptr [edi + 0x10], 0x40000000",
        "jmp dword ptr [{continue_at}]",
        note = sym note_ribbon,
        continue_at = sym NO_RIBBON_CONTINUE,
    )
}

extern "C" fn note_ribbon(had_ribbon: i32) {
    let _ = std::panic::catch_unwind(|| {
        HAD_RIBBON.store(had_ribbon != 0, Ordering::Relaxed);
        OPEN_WANTED.store(false, Ordering::Relaxed);
        log_debug!("Typewriter prompt shown; ink ribbon: {}.", had_ribbon != 0);
    });
}

/// Stands in for `cmp dword ptr [esp+0x60], 1`.
///
/// The instruction after it is `jne`, so what has to come out of here is the
/// zero flag: set to carry on into saving, clear to stop. `popad` leaves flags
/// alone, which is what lets the answer be worked out before the registers are
/// put back.
///
/// # Safety
/// Reached only through the call written over that instruction, so the choice
/// is where the original would have read it, four bytes further up for the
/// return address.
#[unsafe(naked)]
unsafe extern "C" fn choice_stub() {
    core::arch::naked_asm!(
        "pushad",
        // The choice: 0x60 as the original read it, 4 for the return address
        // this call pushed, 0x20 for pushad.
        "push dword ptr [esp + 0x84]",
        "call {decide}",
        "add esp, 4",
        // Zero means carry on, which is what the `jne` below wants.
        "test eax, eax",
        "popad",
        "ret",
        decide = sym decide_choice,
    )
}

/// Answers whether the game should carry on into saving.
extern "C" fn decide_choice(choice: i32) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if choice == CHOICE_NO {
            log_debug!("Typewriter: declined.");
            return 1;
        }

        let had_ribbon = HAD_RIBBON.load(Ordering::Relaxed);

        // With a ribbon there were three choices and the third is the box.
        // Without one, the only question asked was about the box.
        let wants_box = if had_ribbon {
            choice == CHOICE_USE
        } else {
            choice == CHOICE_YES
        };

        OPEN_WANTED.store(wants_box, Ordering::Relaxed);

        if wants_box {
            log_info!("Typewriter: the item box was chosen.");
        }

        // Carry on either way. Saving and opening the box run through the same
        // call, and which one happens is decided there.
        0
    });

    result.unwrap_or(1)
}

/// Stands in for the call that opens a screen.
///
/// The phase is already on the stack when this is reached. Saving is phase 6
/// and the inventory is phase 5, so choosing the box is one decrement.
///
/// # Safety
/// Reached only through the call written over `call set_room_phase`, so the
/// phase sits directly above the return address, and `ecx` is the object that
/// call expected.
#[unsafe(naked)]
unsafe extern "C" fn open_screen_stub() {
    core::arch::naked_asm!(
        "pushad",
        "call {opening}",
        "test eax, eax",
        "popad",
        "je 2f",
        // Phase 6 becomes phase 5: the inventory instead of the save screen.
        "dec dword ptr [esp + 4]",
        "2:",
        "jmp dword ptr [{set_phase}]",
        opening = sym opening_box,
        set_phase = sym SET_ROOM_PHASE,
    )
}

/// Readies the box and says whether the screen about to open is for it.
extern "C" fn opening_box() -> i32 {
    let result = std::panic::catch_unwind(|| {
        if !OPEN_WANTED.swap(false, Ordering::Relaxed) {
            return 0;
        }

        // The inventory screen has its own setup, and the game runs it before
        // setting the phase every other time the screen is opened.
        let holder = INVENTORY_HOLDER.load(Ordering::Relaxed);
        let prepare = PREPARE_INVENTORY.load(Ordering::Relaxed);

        if holder != 0 && prepare != 0 {
            // Safety: the global is written by the game and read the same way
            // here as in the code that opens the inventory normally.
            let inventory = unsafe { (holder as *const usize).read_volatile() };
            if inventory != 0 {
                unsafe { crate::game::call::thiscall0(prepare, inventory) };
            }
        }

        crate::feature::item_box::force_open();
        1
    });

    result.unwrap_or(0)
}

// --- Pointing the game at our own message archive ---

mod message;
