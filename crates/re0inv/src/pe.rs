//! Reads the game module as mapped in memory.
//!
//! Not from disk: the RE0 executable ships with `.text` encrypted by the Steam
//! DRM stub. Only the in-memory image is useful, and only after the stub runs.

use std::slice;

use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameA, GetModuleHandleA};

const DOS_SIGNATURE: u16 = 0x5A4D;
const NT_SIGNATURE: u32 = 0x0000_4550;

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub start: usize,
    pub size: usize,
    pub executable: bool,
    pub writable: bool,
}

impl Section {
    pub fn end(&self) -> usize {
        self.start + self.size
    }

    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.start && addr < self.end()
    }

    /// # Safety
    /// Section must be mapped. True for the main module for the process lifetime.
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

        let nt = base + *((base + 0x3C) as *const i32) as usize;
        if *(nt as *const u32) != NT_SIGNATURE {
            return None;
        }

        let coff = nt + 4;
        let num_sections = *((coff + 2) as *const u16) as usize;
        let size_of_optional = *((coff + 16) as *const u16) as usize;
        let optional = coff + 20;
        let size_of_image = *((optional + 56) as *const u32) as usize;
        let sec_table = optional + size_of_optional;

        let mut sections = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let s = sec_table + i * 40;
            let name = String::from_utf8_lossy(slice::from_raw_parts(s as *const u8, 8))
                .trim_end_matches('\0')
                .to_string();
            let virtual_size = *((s + 8) as *const u32) as usize;
            let virtual_addr = *((s + 12) as *const u32) as usize;
            let characteristics = *((s + 36) as *const u32);

            sections.push(Section {
                name,
                start: base + virtual_addr,
                size: virtual_size,
                executable: characteristics & 0x2000_0000 != 0,
                writable: characteristics & 0x8000_0000 != 0,
            });
        }

        Some(Module { base, size: size_of_image, sections })
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn section_of(&self, addr: usize) -> Option<&Section> {
        self.sections.iter().find(|s| s.contains(addr))
    }

    /// Code section. `.bind` is the DRM stub, never the game's own code.
    pub fn code_section(&self) -> Option<&Section> {
        self.section(".text")
            .or_else(|| self.sections.iter().find(|s| s.executable && s.name != ".bind"))
    }
}

/// Directory holding the game executable. Steam may launch with a different CWD.
pub fn game_directory() -> std::path::PathBuf {
    let mut buf = [0u8; 1024];
    let len = unsafe { GetModuleFileNameA(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
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
