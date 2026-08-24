//! Writing to the game's code pages, reversibly.
//!
//! Code pages are mapped read-execute. Writing to one means changing its
//! protection, writing, and putting the protection back. Leaving a code page
//! writable would be sloppy in a way that hides bugs: a stray write that should
//! have faulted immediately would instead corrupt an instruction and surface
//! much later, somewhere unrelated.
//!
//! Every patch keeps the bytes it replaced, so it can be undone. That is not
//! only for uninstalling: when a patch turns out to be wrong, being able to put
//! the original back is the difference between a diagnosis and a guess.

use std::ffi::c_void;

use crate::core::logging::{log_debug, log_error};
use crate::win32::{
    FlushInstructionCache, GetCurrentProcess, VirtualProtect, PAGE_EXECUTE_READWRITE,
};

/// A write to the game's code, with the bytes it replaced.
pub struct Patch {
    address: usize,
    original: Vec<u8>,
}

impl Patch {
    /// Overwrites `address` with `bytes`, keeping what was there.
    ///
    /// # Safety
    /// `address` must be inside a mapped, executable region of the game module,
    /// with at least `bytes.len()` bytes available. The caller is also
    /// responsible for the patch making sense: this writes whatever it is
    /// given, including bytes that cut an instruction in half.
    pub unsafe fn write(address: usize, bytes: &[u8]) -> Option<Patch> {
        if bytes.is_empty() {
            return None;
        }

        let original = read_bytes(address, bytes.len());

        if !write_protected(address, bytes) {
            log_error!("Failed to write {} bytes at 0x{:08X}.", bytes.len(), address);
            return None;
        }

        log_debug!(
            "Patched 0x{:08X}: {} -> {}",
            address,
            hex(&original),
            hex(bytes)
        );

        Some(Patch { address, original })
    }

    /// Puts the original bytes back.
    ///
    /// # Safety
    /// The region must still be mapped, and nothing else may have patched over
    /// this one in the meantime.
    pub unsafe fn revert(&self) -> bool {
        let restored = write_protected(self.address, &self.original);

        if restored {
            log_debug!("Reverted 0x{:08X}.", self.address);
        } else {
            log_error!("Failed to revert 0x{:08X}.", self.address);
        }

        restored
    }
}

/// Copies bytes out of the game's code.
///
/// A byte-at-a-time volatile copy rather than a slice. A Rust slice over code
/// this mod is about to overwrite, and that the processor is executing, is a
/// promise that nothing else touches it — which is exactly false here.
///
/// # Safety
/// `address` must be readable for `len` bytes.
unsafe fn read_bytes(address: usize, len: usize) -> Vec<u8> {
    (0..len)
        .map(|offset| ((address + offset) as *const u8).read_volatile())
        .collect()
}

/// Makes the page writable, writes, and restores the original protection.
///
/// # Safety
/// `address` must be inside a mapped region with `bytes.len()` bytes available.
unsafe fn write_protected(address: usize, bytes: &[u8]) -> bool {
    let mut previous: u32 = 0;

    let unlocked = VirtualProtect(
        address as *const c_void,
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut previous,
    );
    if unlocked == 0 {
        return false;
    }

    std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());

    // Restore the protection even if the write above was the last thing we
    // wanted to do here. A code page left writable is a latent bug.
    let mut discarded: u32 = 0;
    let relocked = VirtualProtect(
        address as *const c_void,
        bytes.len(),
        previous,
        &mut discarded,
    );

    if relocked == 0 {
        log_error!(
            "Wrote at 0x{:08X} but could not restore page protection 0x{:X}.",
            address,
            previous
        );
    }

    // The CPU may hold a stale copy of instructions it already fetched.
    FlushInstructionCache(GetCurrentProcess(), address as *const c_void, bytes.len());

    true
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}
