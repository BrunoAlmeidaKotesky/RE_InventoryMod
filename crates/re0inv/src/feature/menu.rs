//! Making the inventory screen show the box in the partner's half.
//!
//! # Why this is not one write
//!
//! The screen decides three separate things, at three separate moments, and all
//! three have to agree before the box is usable:
//!
//! 1. **Does the partner half come on screen at all.** Decided by the opening
//!    animation, which takes an id. Id 1 is the one that brings that half in.
//!    Without it the screen is the single-panel one the game shows whenever the
//!    two characters are apart — and at a typewriter they usually are.
//! 2. **Is the half marked as displayed.** `menu+0x2CA`, read by the code that
//!    moves the selection between the two halves.
//! 3. **Is exchanging allowed.** `menu+0x28B`, which the game computes from how
//!    close the partner is. Zero allows it.
//!
//! Forcing the last two from the panel-drawing observer was not enough: by the
//! time anything is drawn, the animation has already decided the partner half is
//! not part of this screen.
//!
//! # Putting it back
//!
//! The exchange field is the game's own state and is restored when the box
//! closes. Leaving it forced would let the player hand items to a partner on the
//! other side of the map.
//!
//! Whose partner it was is remembered too. Swapping to the other character mid
//! screen means the box is no longer the thing in the other half, so the game's
//! own answer comes back for as long as that lasts.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::hook::panel;
use crate::hook::patch::Patch;

/// `inc dword ptr [edi+0x294]`.
const BUMP_PHASE: [u8; 6] = [0xFF, 0x87, 0x94, 0x02, 0x00, 0x00];
/// `cmp byte ptr [edi+0x2CA], 1`.
const TEST_PARTNER_SHOWN: [u8; 7] = [0x80, 0xBF, 0xCA, 0x02, 0x00, 0x00, 0x01];
/// `mov eax, [esi+0x60]; push 0`.
const CLOSING: [u8; 5] = [0x8B, 0x46, 0x60, 0x6A, 0x00];
/// `cmp byte ptr [eax+0x25], 1; je 0x005E50F7`.
const VALIDATION: [u8; 10] = [0x80, 0x78, 0x25, 0x01, 0x0F, 0x84, 0x4F, 0x15, 0x00, 0x00];

static PLAY_ANIMATION: AtomicUsize = AtomicUsize::new(0);
static START_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static CHANGE_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static CLOSE_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static VALID_CONTINUE: AtomicUsize = AtomicUsize::new(0);
static VALID_KICK: AtomicUsize = AtomicUsize::new(0);

/// The partner the box stood in for, so a character swap can be noticed.
static BOX_PARTNER: AtomicUsize = AtomicUsize::new(0);

/// Addresses the handlers need to ask who the partner is.
static CHARACTER_HOLDER: AtomicUsize = AtomicUsize::new(0);
static PARTNER_CHARACTER: AtomicUsize = AtomicUsize::new(0);

pub struct Menu {
    patches: Vec<Patch>,
}

impl Menu {
    /// # Safety
    /// The addresses must belong to the build actually running, and the code
    /// section must be decrypted.
    pub unsafe fn install(addresses: &Addresses) -> Menu {
        let mut patches = Vec::new();

        CHARACTER_HOLDER.store(addresses.character_holder, Ordering::Relaxed);
        PARTNER_CHARACTER.store(addresses.partner_character, Ordering::Relaxed);
        START_CONTINUE.store(addresses.inventory_menu_start + BUMP_PHASE.len(), Ordering::Relaxed);
        CHANGE_CONTINUE.store(
            addresses.inventory_change_character + TEST_PARTNER_SHOWN.len(),
            Ordering::Relaxed,
        );
        CLOSE_CONTINUE.store(addresses.inventory_menu_close + CLOSING.len(), Ordering::Relaxed);

        install_animation(addresses, &mut patches);

        crate::hook::detour::jump_over(
            &mut patches,
            "the screen finishing its setup",
            addresses.inventory_menu_start,
            &BUMP_PHASE,
            start_stub as unsafe extern "C" fn() as usize,
        );

        crate::hook::detour::jump_over(
            &mut patches,
            "the played character changing",
            addresses.inventory_change_character,
            &TEST_PARTNER_SHOWN,
            change_stub as unsafe extern "C" fn() as usize,
        );

        crate::hook::detour::jump_over(
            &mut patches,
            "the screen closing",
            addresses.inventory_menu_close,
            &CLOSING,
            close_stub as unsafe extern "C" fn() as usize,
        );

        VALID_CONTINUE.store(addresses.partner_half_continue, Ordering::Relaxed);
        VALID_KICK.store(addresses.partner_half_kick, Ordering::Relaxed);

        crate::hook::detour::jump_over(
            &mut patches,
            "the partner-half validation",
            addresses.partner_half_valid,
            &VALIDATION,
            valid_stub as unsafe extern "C" fn() as usize,
        );

        log_info!("Inventory screen: {} patch(es) applied.", patches.len());

        Menu { patches }
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

/// Redirects the call that plays the opening animation.
///
/// The address it went to is read out of the game rather than taken from the
/// table, so a table that has drifted shows up as a refusal here instead of as
/// a jump into the middle of something else.
unsafe fn install_animation(addresses: &Addresses, patches: &mut Vec<Patch>) {
    let Some(target) = crate::hook::detour::call_target(addresses.inventory_open_animation) else {
        log_warn!("The inventory does not open with a call where expected.");
        return;
    };

    if target != addresses.play_menu_animation {
        log_warn!(
            "The inventory opens by calling 0x{target:08X}, not the animation we know about."
        );
        return;
    }

    PLAY_ANIMATION.store(target, Ordering::Relaxed);

    let expected =
        crate::hook::detour::call_bytes(addresses.inventory_open_animation, target);
    let bytes = crate::hook::detour::call_bytes(
        addresses.inventory_open_animation,
        animation_stub as unsafe extern "C" fn() as usize,
    );

    match Patch::write_expecting(addresses.inventory_open_animation, &expected, &bytes) {
        Some(patch) => patches.push(patch),
        None => log_warn!("Could not redirect the opening animation."),
    }
}

// --- The trampolines ---

/// Chooses which opening animation runs.
///
/// # Safety
/// Reached only through the call written over `call play_menu_animation`, so
/// `edi` is the menu and the animation's id sits above the return address.
#[unsafe(naked)]
unsafe extern "C" fn animation_stub() {
    core::arch::naked_asm!(
        "pushad",
        "push edi",
        "call {opening}",
        "add esp, 4",
        "test eax, eax",
        "popad",
        "je 2f",
        // The id the animation was going to be given, replaced with the one
        // that brings the partner half on screen.
        "mov dword ptr [esp + 4], 1",
        "2:",
        "jmp dword ptr [{play}]",
        opening = sym opening_screen,
        play = sym PLAY_ANIMATION,
    )
}

extern "C" fn opening_screen(menu: usize) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if !crate::feature::item_box::is_open() {
            return 0;
        }

        // Safety: the game is opening this screen right now, so the object is
        // alive and this is its own field.
        unsafe { panel::mark_partner_shown(menu) };

        log_debug!("Opening the inventory with the partner half on screen.");
        1
    });

    result.unwrap_or(0)
}

/// Runs where the screen finishes setting itself up.
///
/// # Safety
/// Reached only through the jump written over `inc [edi+0x294]`, so `edi` is
/// the menu. That instruction is re-executed here.
#[unsafe(naked)]
unsafe extern "C" fn start_stub() {
    core::arch::naked_asm!(
        "pushad",
        "push edi",
        "call {setup}",
        "add esp, 4",
        "popad",
        "inc dword ptr [edi + 0x294]",
        "jmp dword ptr [{continue_at}]",
        setup = sym screen_ready,
        continue_at = sym START_CONTINUE,
    )
}

extern "C" fn screen_ready(menu: usize) {
    let _ = std::panic::catch_unwind(|| {
        if !crate::feature::item_box::is_open() {
            return;
        }

        // Whoever the partner is now is who the box is standing in for.
        BOX_PARTNER.store(partner(), Ordering::Relaxed);

        // Safety: the screen has just finished setting itself up.
        unsafe {
            panel::mark_partner_shown(menu);
            panel::allow_exchange(menu);
        }

        log_debug!("Inventory ready with the box in the partner half.");
    });
}

/// Runs when the played character changes while the screen is up.
///
/// # Safety
/// Reached only through the jump written over `cmp [edi+0x2CA], 1`, so `edi` is
/// the menu. That comparison is re-executed last, because the `jne` after it
/// reads the flags it sets, and `popad` leaves flags alone.
#[unsafe(naked)]
unsafe extern "C" fn change_stub() {
    core::arch::naked_asm!(
        "pushad",
        "push edi",
        "call {changed}",
        "add esp, 4",
        "popad",
        "cmp byte ptr [edi + 0x2CA], 1",
        "jmp dword ptr [{continue_at}]",
        changed = sym character_changed,
        continue_at = sym CHANGE_CONTINUE,
    )
}

extern "C" fn character_changed(menu: usize) {
    let _ = std::panic::catch_unwind(|| {
        if !crate::feature::item_box::is_open() {
            return;
        }

        let still_ours = partner() == BOX_PARTNER.load(Ordering::Relaxed);

        // Safety: the screen is up, so the object is alive.
        unsafe {
            if still_ours {
                panel::allow_exchange(menu);
            } else {
                // The other half is a real character again — the one the box
                // was standing in for. The box closes outright: half-open, the
                // accessors would keep answering questions about that
                // character with the box's contents.
                crate::feature::item_box::abandon();
                panel::restore_exchange(menu);
            }
        }

        log_debug!("Played character changed; the box is {}.", if still_ours { "still in the other half" } else { "closed" });
    });
}

/// Stands in for the check that throws the selection out of the partner half.
///
/// The game re-validates every frame that the partner is still there to
/// exchange with, and kicks the selection back to the played half when not.
/// Right behaviour for a partner, who can walk away — this is why the box
/// worked exactly when the partner happened to be standing close, and read as
/// broken the rest of the time. A box cannot walk away, so while it is the
/// thing in that half the validation always passes.
///
/// # Safety
/// Reached only through the jump written over the compare-and-branch pair, so
/// `eax` holds the object whose byte was being tested, and `eax` is dead after
/// the pair — the code at the continuation reloads it immediately.
#[unsafe(naked)]
unsafe extern "C" fn valid_stub() {
    core::arch::naked_asm!(
        "pushad",
        "mov ebp, esp",
        "and esp, -16",
        "sub esp, 16",
        "mov [esp], eax",
        "call {decide}",
        "mov esp, ebp",
        // Flags survive popad, so the branch below still sees this test.
        "test eax, eax",
        "popad",
        "jnz 2f",
        "jmp dword ptr [{continue_at}]",
        "2:",
        "jmp dword ptr [{kick}]",
        decide = sym still_valid,
        continue_at = sym VALID_CONTINUE,
        kick = sym VALID_KICK,
    )
}

/// Whether the selection should be thrown out of the partner half.
extern "C" fn still_valid(unit: usize) -> i32 {
    let result = std::panic::catch_unwind(|| {
        // The box does not walk away.
        if crate::feature::item_box::is_open() {
            return 0;
        }

        // The game's own check, reproduced: byte 0x25 set means gone.
        match crate::debug::memory::read_array::<1>(unit + 0x25) {
            Some([1]) => 1,
            _ => 0,
        }
    });

    result.unwrap_or(0)
}

/// Runs as the screen closes.
///
/// # Safety
/// Reached only through the jump written over the two instructions re-executed
/// here.
#[unsafe(naked)]
unsafe extern "C" fn close_stub() {
    core::arch::naked_asm!(
        "pushad",
        "call {closing}",
        "popad",
        "mov eax, [esi + 0x60]",
        "push 0",
        "jmp dword ptr [{continue_at}]",
        closing = sym screen_closing,
        continue_at = sym CLOSE_CONTINUE,
    )
}

extern "C" fn screen_closing() {
    let _ = std::panic::catch_unwind(crate::feature::item_box::close_with_menu);
}

/// Who the partner character is, or zero when there is nobody.
fn partner() -> usize {
    let holder = CHARACTER_HOLDER.load(Ordering::Relaxed);
    let getter = PARTNER_CHARACTER.load(Ordering::Relaxed);

    if holder == 0 || getter == 0 {
        return 0;
    }

    // Safety: the global is written by the game and read the same way here as
    // in the code being borrowed from.
    let holder = unsafe { (holder as *const usize).read_volatile() };
    if holder == 0 {
        return 0;
    }

    unsafe { crate::game::call::thiscall0(getter, holder) }
}
