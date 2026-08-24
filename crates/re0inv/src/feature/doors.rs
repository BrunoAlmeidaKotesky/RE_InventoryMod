//! Skipping the animation the game plays when walking through a door.
//!
//! # What the transition is
//!
//! Not a video and not a disguised loading screen. `sDoorLoad::update` at
//! `0x00552300` is a six-state machine, and the middle of it plays a door model
//! with its own camera while the room changes behind it. Roughly three and a
//! third seconds, of which about two are the animation and the rest are fades.
//!
//! # Why shortening the timer is safe
//!
//! State four does not end when the timer runs out. It ends when
//! `[this+0x28]` is set, and that only happens once `0x00551CC0` — the function
//! that actually performs the room change — reports it is finished. If the
//! timer expires first, the game puts a fresh second back on it and waits
//! again.
//!
//! So the timer is not racing the loader. It is padding on top of it, and
//! removing the padding cannot show an unloaded room. The handshake and the
//! function behind it are deliberately left alone; those *would* race.
//!
//! # The values matter
//!
//! Two of them soft-lock the game, and both are reachable by accident:
//!
//! - `0.0` makes the comparison at `0x0055250D` match, which skips the whole
//!   block. `0x00551CC0` is then never called, `[this+0x28]` is never set, and
//!   state four never ends.
//! - `-1.0` lands in the re-arm at `0x0055255C`, which writes a second back
//!   into the timer. The animation restarts instead of ending, which is why the
//!   mod this borrows the idea from only ever shortened it.
//!
//! A small positive value avoids both: the timer goes negative on the first
//! frame, the room change is asked about every frame after that, and the
//! transition ends on the frame after it reports done.

use crate::core::logging::{log_info, log_warn};
use crate::game::addresses::Addresses;
use crate::hook::patch::Patch;

/// What the animation timer is set to instead of two seconds.
///
/// Small and positive. Not zero and not negative; see the module notes.
const BRIEF: f32 = 0.001;

/// What the fades are shortened to.
///
/// Not zero either. States one and five wait on a flag the fade sets when it
/// completes, and whether a zero-length fade ever sets that flag is unknown. A
/// fade that never completes is the same soft-lock by another route.
const QUICK_FADE: f32 = 0.05;

/// `mov dword ptr [edi+0x2C], imm32`.
const MOV_TIMER: [u8; 3] = [0xC7, 0x47, 0x2C];
/// `movss xmm0, [edi+0x2C]`.
const RELOAD_XMM0: [u8; 5] = [0xF3, 0x0F, 0x10, 0x47, 0x2C];
/// `jmp short +0x1C`, landing on the instruction after the replaced block.
const SKIP_TO_TAIL: [u8; 2] = [0xEB, 0x1C];
const NOP: u8 = 0x90;

/// Everything this feature changed, so it can all be put back.
pub struct Doors {
    patches: Vec<Patch>,
}

impl Doors {
    /// Applies the patches. Every one verifies what it is overwriting first.
    ///
    /// A patch that does not match is skipped rather than forced, and the rest
    /// still apply: a partly shortened transition is a cosmetic disappointment,
    /// while writing over an instruction that is not what we think it is
    /// corrupts the game.
    ///
    /// # Safety
    /// The addresses must belong to the build actually running, and the code
    /// section must be decrypted.
    pub unsafe fn install(addresses: &Addresses, shorten_fades: bool) -> Doors {
        let mut patches = Vec::new();

        apply_timer(addresses, &mut patches);

        if shorten_fades {
            apply_fades(addresses, &mut patches);
        }

        log_info!(
            "Door skip: {} patch(es) applied{}.",
            patches.len(),
            if shorten_fades { "" } else { ", fades left alone" }
        );

        Doors { patches }
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

/// Replaces the three places the animation timer is set.
unsafe fn apply_timer(addresses: &Addresses, patches: &mut Vec<Patch>) {
    // The computed duration: frame time, times two, times a sixtieth, times the
    // motion's frame count, plus two seconds.
    const COMPUTED_DURATION: [u8; 14] = [
        0xF3, 0x0F, 0x10, 0x40, 0x38, // movss xmm0, [eax+0x38]
        0xF3, 0x0F, 0x59, 0x05, 0x14, 0xA4, 0xCB, 0x00, // mulss xmm0, [0xCBA414]
        0xF3, // the first byte of the next instruction
    ];

    // What the rest of that computation occupies, which becomes padding.
    const COMPUTATION_TAIL: [u8; 28] = [
        0x0F, 0x59, 0x05, 0x24, 0xBE, 0xCC, 0x00, // rest of mulss [0xCCBE24]
        0xF3, 0x0F, 0x59, 0x81, 0xF4, 0x04, 0x00, 0x00, // mulss xmm0, [ecx+0x4F4]
        0xF3, 0x0F, 0x58, 0x05, 0x14, 0xA4, 0xCB, 0x00, // addss xmm0, [0xCBA414]
        0xF3, 0x0F, 0x11, 0x47, 0x2C, // movss [edi+0x2C], xmm0
    ];

    /// `mov dword ptr [edi+0x2C], 1.0f`, used by two separate branches.
    const SET_ONE_SECOND: [u8; 7] = [0xC7, 0x47, 0x2C, 0x00, 0x00, 0x80, 0x3F];

    let brief = BRIEF.to_le_bytes();

    // Replace the whole computation with a constant, then jump over what is
    // left of it. The reload of xmm0 keeps the register agreeing with the field
    // for anything downstream that reads it.
    let mut replacement = Vec::with_capacity(COMPUTED_DURATION.len());
    replacement.extend_from_slice(&MOV_TIMER);
    replacement.extend_from_slice(&brief);
    replacement.extend_from_slice(&RELOAD_XMM0);
    replacement.extend_from_slice(&SKIP_TO_TAIL);

    push(
        patches,
        "door animation duration",
        Patch::write_expecting(addresses.door_duration, &COMPUTED_DURATION, &replacement),
    );

    push(
        patches,
        "door animation duration tail",
        Patch::write_expecting(
            addresses.door_duration_tail,
            &COMPUTATION_TAIL,
            &[NOP; COMPUTATION_TAIL.len()],
        ),
    );

    // The branch taken when the door has no model to play.
    let mut modelless = SET_ONE_SECOND;
    modelless[3..].copy_from_slice(&brief);
    push(
        patches,
        "door duration without a model",
        Patch::write_expecting(addresses.door_duration_modelless, &SET_ONE_SECOND, &modelless),
    );

    // The re-arm. Without this one the transition restarts rather than ending,
    // which is exactly the half-finished result this feature exists to fix.
    let mut rearm = SET_ONE_SECOND;
    rearm[3..].copy_from_slice(&brief);
    push(
        patches,
        "door duration re-arm",
        Patch::write_expecting(addresses.door_duration_rearm, &SET_ONE_SECOND, &rearm),
    );
}

/// Shortens the four fades around the transition.
///
/// These hide the transition rather than removing it. Without them the wait is
/// gone but the screen still fades out and back in around a glimpse of the door.
unsafe fn apply_fades(addresses: &Addresses, patches: &mut Vec<Patch>) {
    const HALF_SECOND: [u8; 4] = [0x00, 0x00, 0x00, 0x3F];
    const THREE_TENTHS: [u8; 4] = [0x9A, 0x99, 0x99, 0x3E];

    let quick = QUICK_FADE.to_le_bytes();

    for (name, address, expected) in [
        ("screen fade out", addresses.door_fade_screen_out, HALF_SECOND),
        ("audio fade out", addresses.door_fade_audio_out, HALF_SECOND),
        ("fade in over the door", addresses.door_fade_in, HALF_SECOND),
        ("final fade", addresses.door_fade_final, THREE_TENTHS),
    ] {
        push(
            patches,
            name,
            Patch::write_expecting(address, &expected, &quick),
        );
    }
}

fn push(patches: &mut Vec<Patch>, what: &str, patch: Option<Patch>) {
    match patch {
        Some(patch) => patches.push(patch),
        None => log_warn!("Door skip: could not patch the {what}."),
    }
}
