//! Waits out the Steam DRM decryption, then dumps the code section to disk.
//!
//! The stub lives in `.bind` and runs at the executable's entry point, which is
//! after the loader has already called DllMain for every imported DLL. Anything
//! that reads `.text` earlier than that reads ciphertext.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::core::logging::{log_debug, log_info, log_warn};
use crate::game::module::{count_bytes, Section};

/// MSVC inter-function padding (`int3`). Real x86 code contains thousands of
/// these; high-entropy ciphertext contains effectively none, so there is no
/// grey zone between the two cases and the threshold can be generous.
const INT3_PADDING: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xCC];
const PADDING_THRESHOLD: usize = 64;
const SAMPLE_BYTES: usize = 1024 * 1024;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// # Safety
/// The section must be mapped and readable.
pub unsafe fn looks_decrypted(section: &Section) -> bool {
    let bytes = section.as_slice();
    let sample = &bytes[..SAMPLE_BYTES.min(bytes.len())];
    count_bytes(sample, &INT3_PADDING) >= PADDING_THRESHOLD
}

/// Blocks until the section looks like code, or the timeout expires.
/// Call from a dedicated thread, never from DllMain.
pub fn wait_until_decrypted(section: &Section, timeout: Duration) -> bool {
    let started = Instant::now();
    let mut announced = false;

    loop {
        if unsafe { looks_decrypted(section) } {
            log_debug!(
                "Code section decrypted after {} ms.",
                started.elapsed().as_millis()
            );
            return true;
        }

        if !announced {
            log_info!("Waiting for the Steam DRM stub to decrypt the code section...");
            announced = true;
        }

        if started.elapsed() >= timeout {
            log_warn!(
                "Timed out after {} s; the code section still looks encrypted.",
                timeout.as_secs()
            );
            return false;
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Writes the section verbatim, with no PE header: file byte 0 corresponds to
/// `section.start`, so the analysis tool needs that base to rebuild addresses.
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
