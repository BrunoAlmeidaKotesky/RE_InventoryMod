//! Redirecting a game function to one of ours.
//!
//! A detour writes a five-byte relative jump over the start of the target. From
//! then on, calling the game function lands in the replacement instead.
//!
//! This only works for functions we replace outright. Calling the original from
//! the replacement would need the overwritten instructions relocated somewhere
//! else first, and that is a decoder's job, not a memcpy's. Every function
//! hooked here has semantics simple enough to reimplement completely: find an
//! empty slot, count empty slots, search for an item.

use crate::core::logging::{log_error, log_info};
use crate::hook::patch::Patch;

/// `jmp rel32`.
const OP_JMP_REL32: u8 = 0xE9;
/// Opcode plus the four displacement bytes.
pub const JUMP_LENGTH: usize = 5;

/// Builds the bytes for a jump from `from` to `to`.
///
/// The displacement is measured from the end of the jump instruction, not its
/// start, because that is where the processor has advanced to by the time it
/// applies the offset.
///
/// Returns `None` if the two addresses are further apart than a 32-bit signed
/// displacement can reach. Inside one 32-bit process that cannot happen, but
/// returning the failure is cheaper than reasoning about why it cannot.
pub fn jump_bytes(from: usize, to: usize) -> Option<[u8; JUMP_LENGTH]> {
    let next = (from as i64) + JUMP_LENGTH as i64;
    let displacement = (to as i64) - next;

    let displacement = i32::try_from(displacement).ok()?;
    let offset = displacement.to_le_bytes();

    Some([
        OP_JMP_REL32,
        offset[0],
        offset[1],
        offset[2],
        offset[3],
    ])
}

/// A game function redirected to a replacement.
pub struct Detour {
    name: &'static str,
    patch: Patch,
}

impl Detour {
    /// Redirects `target` to `replacement`.
    ///
    /// # Safety
    /// `target` must be the entry point of a game function whose calling
    /// convention the replacement matches exactly, including how the stack is
    /// cleaned up. Getting that wrong corrupts the caller's stack, and the crash
    /// happens somewhere else entirely.
    ///
    /// The first five bytes of the target are destroyed, so the replacement must
    /// not need to call the original.
    pub unsafe fn install(name: &'static str, target: usize, replacement: usize) -> Option<Detour> {
        let Some(bytes) = jump_bytes(target, replacement) else {
            log_error!("{name}: 0x{target:08X} is out of jump range from 0x{replacement:08X}.");
            return None;
        };

        let patch = Patch::write(target, &bytes)?;
        log_info!("Hooked {name} at 0x{target:08X}.");

        Some(Detour { name, patch })
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// # Safety
    /// See `Patch::revert`.
    pub unsafe fn remove(&self) -> bool {
        self.patch.revert()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forward_jump_encodes_the_distance_past_the_instruction() {
        // From 0x1000 to 0x1010: the processor is at 0x1005 when it applies the
        // offset, so the displacement is 0x0B, not 0x10.
        let bytes = jump_bytes(0x1000, 0x1010).unwrap();
        assert_eq!(bytes, [0xE9, 0x0B, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn a_backward_jump_encodes_a_negative_displacement() {
        let bytes = jump_bytes(0x1010, 0x1000).unwrap();
        assert_eq!(bytes[0], OP_JMP_REL32);

        let displacement = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        assert_eq!(displacement, -0x15);
    }

    #[test]
    fn a_jump_to_the_instruction_after_itself_has_no_displacement() {
        let bytes = jump_bytes(0x1000, 0x1005).unwrap();
        assert_eq!(bytes, [0xE9, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn round_tripping_the_displacement_lands_on_the_target() {
        for (from, to) in [
            (0x0040_1000usize, 0x0050_0000usize),
            (0x0050_0000, 0x0040_1000),
            (0x004D_B480, 0x1000_0000),
        ] {
            let bytes = jump_bytes(from, to).unwrap();
            let displacement = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
            let landed = (from as i64 + JUMP_LENGTH as i64 + displacement as i64) as usize;
            assert_eq!(landed, to);
        }
    }

    #[test]
    fn an_unreachable_target_is_refused_rather_than_truncated() {
        // Further apart than a signed 32-bit displacement can express.
        assert!(jump_bytes(0, u64::MAX as usize).is_none());
    }
}
