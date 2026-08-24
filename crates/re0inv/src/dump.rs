//! Waits for the Steam DRM stub to decrypt `.text`, then dumps it to disk.
//!
//! The stub lives in `.bind` and runs at the executable entry point, which is
//! after the loader has already run DllMain for every imported DLL. Reading
//! `.text` any earlier yields ciphertext.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::log::{log_debug, log_info, log_warn};
use crate::pe::{count_bytes, Section};

/// MSVC inter-function padding (`int3`). Real x86 code has thousands of these;
/// high-entropy ciphertext has effectively none, so there is no grey zone.
const INT3_PADDING: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xCC];
const PADDING_THRESHOLD: usize = 64;
const SAMPLE_SIZE: usize = 1024 * 1024;

/// # Safety
/// Section must be mapped and readable.
pub unsafe fn looks_decrypted(section: &Section) -> bool {
    let bytes = section.as_slice();
    let sample = &bytes[..SAMPLE_SIZE.min(bytes.len())];
    count_bytes(sample, &INT3_PADDING) >= PADDING_THRESHOLD
}

/// Blocks until the section looks like code, or the timeout expires.
/// Call from a dedicated thread, never from DllMain.
pub fn wait_until_decrypted(section: &Section, timeout: Duration) -> bool {
    let started = Instant::now();
    let mut reported = false;

    loop {
        if unsafe { looks_decrypted(section) } {
            log_debug!("Code section decrypted after {} ms.", started.elapsed().as_millis());
            return true;
        }

        if !reported {
            log_info!("Waiting for the Steam DRM stub to decrypt the code section...");
            reported = true;
        }

        if started.elapsed() >= timeout {
            log_warn!(
                "Timed out after {} s; the code section still looks encrypted.",
                timeout.as_secs()
            );
            return false;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Writes the section verbatim. No PE header: file byte 0 maps to `section.start`.
pub fn dump_section(section: &Section, path: &Path) -> std::io::Result<()> {
    let bytes = unsafe { section.as_slice() };

    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.flush()?;

    log_info!(
        "Dumped '{}' to {} ({} bytes, base 0x{:08X}).",
        section.name,
        path.display(),
        bytes.len(),
        section.start
    );

    Ok(())
}
