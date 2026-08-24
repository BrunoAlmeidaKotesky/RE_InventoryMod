//! Safe primitives for reading and searching the process's own memory.
//!
//! Every read goes through ReadProcessMemory rather than a raw dereference: an
//! unmapped or protected page then returns an error instead of faulting, which
//! matters when scanning addresses that were only guessed at.

use crate::win32::{
    GetCurrentProcess, MemoryBasicInformation, ReadProcessMemory, VirtualQuery, MEM_COMMIT,
    PAGE_GUARD, PAGE_READABLE, PAGE_WRITABLE,
};

/// Bottom of user address space; nothing valid is mapped below this.
const USER_MIN: usize = 0x0001_0000;
/// Top of user address space for a 32-bit process.
const USER_MAX: usize = 0x7FFF_0000;

/// Chunk size used when streaming a region through a scan.
const CHUNK: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct Region {
    pub base: usize,
    pub size: usize,
    pub protect: u32,
    pub writable: bool,
}

impl Region {
    pub fn end(&self) -> usize {
        self.base + self.size
    }
}

/// Reads `out.len()` bytes. Returns false if the range is not fully readable.
pub fn read(addr: usize, out: &mut [u8]) -> bool {
    let mut got: usize = 0;
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            addr as *const _,
            out.as_mut_ptr() as *mut _,
            out.len(),
            &mut got,
        )
    };
    ok != 0 && got == out.len()
}

pub fn read_i32(addr: usize) -> Option<i32> {
    let mut buf = [0u8; 4];
    read(addr, &mut buf).then(|| i32::from_le_bytes(buf))
}

pub fn read_array<const N: usize>(addr: usize) -> Option<[u8; N]> {
    let mut buf = [0u8; N];
    read(addr, &mut buf).then_some(buf)
}

/// Walks the address space and returns every committed, readable region.
pub fn regions() -> Vec<Region> {
    let mut out = Vec::new();
    let mut addr = USER_MIN;

    while addr < USER_MAX {
        let mut info = MemoryBasicInformation::default();
        let written = unsafe {
            VirtualQuery(
                addr as *const _,
                &mut info,
                std::mem::size_of::<MemoryBasicInformation>(),
            )
        };
        if written == 0 {
            break;
        }

        let base = info.base_address as usize;
        let size = info.region_size;

        let readable = info.state == MEM_COMMIT
            && info.protect & PAGE_GUARD == 0
            && info.protect & PAGE_READABLE != 0;

        if readable && size > 0 {
            out.push(Region {
                base,
                size,
                protect: info.protect,
                writable: info.protect & PAGE_WRITABLE != 0,
            });
        }

        // Guard against a query that fails to advance, which would spin forever.
        let next = base.saturating_add(size);
        if next <= addr {
            break;
        }
        addr = next;
    }

    out
}

/// Finds every 4-byte-aligned occurrence of `value`, stopping at `limit` hits.
pub fn scan_i32(value: i32, limit: usize) -> Vec<usize> {
    let needle = value.to_le_bytes();
    let mut hits = Vec::new();
    let mut buf = vec![0u8; CHUNK];

    for region in regions() {
        let mut offset = 0usize;

        while offset < region.size {
            let len = buf.len().min(region.size - offset);
            let chunk = &mut buf[..len];

            // A region can change protection between the query and the read.
            // Skipping is correct: it was not scannable memory anyway.
            if !read(region.base + offset, chunk) {
                offset += len;
                continue;
            }

            let mut i = 0usize;
            while i + 4 <= len {
                if chunk[i..i + 4] == needle {
                    hits.push(region.base + offset + i);
                    if hits.len() >= limit {
                        return hits;
                    }
                }
                i += 4;
            }

            offset += len;
        }
    }

    hits
}

/// Keeps only the addresses that now hold `value`.
pub fn narrow(previous: &[usize], value: i32) -> Vec<usize> {
    previous
        .iter()
        .copied()
        .filter(|&addr| read_i32(addr) == Some(value))
        .collect()
}
