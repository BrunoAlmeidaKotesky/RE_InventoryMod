//! The game module as mapped in memory.
//!
//! Never read from disk: the RE0 executable ships with `.text` encrypted by the
//! Steam DRM stub, so only the in-memory image is meaningful, and only after
//! the stub has run.

use std::slice;

use crate::win32::{GetModuleFileNameA, GetModuleHandleA};

const DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
const NT_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

/// Section added by the Steam DRM packer. Its presence means `.text` starts
/// encrypted and its entry point runs before the game's own.
pub const DRM_SECTION: &str = ".bind";

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    /// Runtime virtual address, module base already applied.
    pub start: usize,
    /// Virtual size, i.e. what is mapped, not the on-disk size.
    pub size: usize,
    pub executable: bool,
    pub writable: bool,
}

impl Section {
    pub fn end(&self) -> usize {
        self.start + self.size
    }

    /// # Safety
    /// The section must still be mapped. True for the main module for the
    /// entire process lifetime.
    pub unsafe fn as_slice(&self) -> &[u8] {
        slice::from_raw_parts(self.start as *const u8, self.size)
    }
}

pub struct Module {
    pub base: usize,
    pub size: usize,
    pub sections: Vec<Section>,
}

impl Module {
    /// Reads the process's own main module, i.e. `re0hd.exe`.
    pub fn current_process() -> Option<Module> {
        let base = unsafe { GetModuleHandleA(std::ptr::null()) } as usize;
        if base == 0 {
            return None;
        }
        unsafe { Module::from_base(base) }
    }

    /// # Safety
    /// `base` must point at a mapped PE image.
    unsafe fn from_base(base: usize) -> Option<Module> {
        if *(base as *const u16) != DOS_SIGNATURE {
            return None;
        }

        // e_lfanew at +0x3C points at the NT header.
        let nt = base + *((base + 0x3C) as *const i32) as usize;
        if *(nt as *const u32) != NT_SIGNATURE {
            return None;
        }

        let coff = nt + 4;
        let section_count = *((coff + 2) as *const u16) as usize;
        let optional_size = *((coff + 16) as *const u16) as usize;

        let optional = coff + 20;
        let image_size = *((optional + 56) as *const u32) as usize;

        // The section table follows the optional header directly.
        let table = optional + optional_size;

        let mut sections = Vec::with_capacity(section_count);
        for i in 0..section_count {
            sections.push(read_section_header(base, table + i * 40));
        }

        Some(Module {
            base,
            size: image_size,
            sections,
        })
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The game's code section. `.bind` is excluded: that is the DRM stub.
    pub fn code(&self) -> Option<&Section> {
        self.section(".text").or_else(|| {
            self.sections
                .iter()
                .find(|s| s.executable && s.name != DRM_SECTION)
        })
    }

    pub fn is_drm_packed(&self) -> bool {
        self.section(DRM_SECTION).is_some()
    }
}

/// # Safety
/// `header` must point at a 40-byte IMAGE_SECTION_HEADER.
unsafe fn read_section_header(base: usize, header: usize) -> Section {
    let name = String::from_utf8_lossy(slice::from_raw_parts(header as *const u8, 8))
        .trim_end_matches('\0')
        .to_string();

    let virtual_size = *((header + 8) as *const u32) as usize;
    let virtual_addr = *((header + 12) as *const u32) as usize;
    let characteristics = *((header + 36) as *const u32);

    Section {
        name,
        start: base + virtual_addr,
        size: virtual_size,
        executable: characteristics & IMAGE_SCN_MEM_EXECUTE != 0,
        writable: characteristics & IMAGE_SCN_MEM_WRITE != 0,
    }
}

/// Directory holding the game executable. Config and log live here; the working
/// directory is unreliable because Steam may launch the game from elsewhere.
pub fn game_directory() -> std::path::PathBuf {
    let mut buf = [0u8; 1024];
    let len =
        unsafe { GetModuleFileNameA(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) };

    if len == 0 {
        return std::path::PathBuf::from(".");
    }

    let path = String::from_utf8_lossy(&buf[..len as usize]).to_string();
    std::path::PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn count_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack.windows(needle.len()).filter(|w| *w == needle).count()
}

/// # Safety
/// `addr` must be readable.
pub unsafe fn read_cstr(addr: usize, max: usize) -> String {
    let mut out = Vec::new();
    for i in 0..max {
        let b = *((addr + i) as *const u8);
        if b == 0 {
            break;
        }
        out.push(b);
    }
    String::from_utf8_lossy(&out).to_string()
}
