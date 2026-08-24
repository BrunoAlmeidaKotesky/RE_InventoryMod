//! Game build identification.
//!
//! Capcom leaves a build stamp in read-only data. Unlike `.text`, that section
//! is never encrypted, so this works even before the DRM stub has run.

use super::module::{find_bytes, read_cstr, Module};

const MARKER: &[u8] = b"MasterRelease ";
const MAX_LEN: usize = 64;

/// The build this project's addresses were gathered against.
pub const SUPPORTED: &str = "MasterRelease Jan 28 2025 16:45:59";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub stamp: String,
}

impl Build {
    pub fn detect(module: &Module) -> Option<Build> {
        let rdata = module.section(".rdata")?;
        let offset = find_bytes(unsafe { rdata.as_slice() }, MARKER)?;
        let stamp = unsafe { read_cstr(rdata.start + offset, MAX_LEN) };

        if stamp.is_empty() {
            None
        } else {
            Some(Build { stamp })
        }
    }

    pub fn is_supported(&self) -> bool {
        self.stamp == SUPPORTED
    }
}
