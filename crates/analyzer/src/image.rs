//! The dumped code section, plus address translation.
//!
//! The dump is raw bytes with no PE header: file byte 0 is the first byte of
//! the section, which lived at `base` in the running process. Every address the
//! tool prints is a runtime virtual address, so it can be compared directly
//! against a debugger or against notes.

use std::path::Path;

pub struct Image {
    pub base: u64,
    pub bytes: Vec<u8>,
}

impl Image {
    pub fn load(path: &Path, base: u64) -> std::io::Result<Image> {
        Ok(Image {
            base,
            bytes: std::fs::read(path)?,
        })
    }

    pub fn end(&self) -> u64 {
        self.base + self.bytes.len() as u64
    }

    pub fn contains(&self, va: u64) -> bool {
        (self.base..self.end()).contains(&va)
    }

    pub fn offset_of(&self, va: u64) -> Option<usize> {
        self.contains(va).then(|| (va - self.base) as usize)
    }

    /// Bytes from `va` to the end of the image.
    pub fn from(&self, va: u64) -> Option<&[u8]> {
        self.offset_of(va).map(|o| &self.bytes[o..])
    }

    /// At most `len` bytes starting at `va`.
    pub fn slice(&self, va: u64, len: usize) -> Option<&[u8]> {
        let offset = self.offset_of(va)?;
        let end = (offset + len).min(self.bytes.len());
        Some(&self.bytes[offset..end])
    }
}
