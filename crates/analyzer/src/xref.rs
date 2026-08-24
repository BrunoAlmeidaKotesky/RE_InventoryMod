//! Cross-reference scanning.
//!
//! There is no symbol table and linear disassembly of a 9 MB section drifts out
//! of alignment wherever data is embedded in code. So references are found by
//! byte pattern instead, then confirmed by decoding at the candidate site.
//!
//! This finds direct calls and jumps. Indirect calls through a vtable or
//! function pointer are invisible here; a function that is never directly
//! called but is clearly used is a hint that it is virtual.

use crate::disasm;
use crate::image::Image;

/// `call rel32`.
const OP_CALL_REL32: u8 = 0xE8;
/// `jmp rel32`. Shows up as a tail call.
const OP_JMP_REL32: u8 = 0xE9;

const REL32_INSTRUCTION_LEN: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Call,
    Jump,
    /// The address appears as a literal 4-byte value, e.g. `mov eax, [addr]`
    /// or a pointer stored in a table.
    Data,
}

#[derive(Debug, Clone)]
pub struct Xref {
    pub from: u64,
    pub kind: RefKind,
}

/// Finds every `call`/`jmp rel32` whose target is `target`.
///
/// A byte that happens to be 0xE8 inside data produces a false positive. Those
/// are rare and obvious once the site is disassembled, which is why the caller
/// is expected to look at each hit rather than trust the count.
pub fn code_refs(image: &Image, target: u64) -> Vec<Xref> {
    let mut out = Vec::new();
    let bytes = &image.bytes;

    if bytes.len() < 5 {
        return out;
    }

    for i in 0..bytes.len() - 5 {
        let opcode = bytes[i];
        let kind = match opcode {
            OP_CALL_REL32 => RefKind::Call,
            OP_JMP_REL32 => RefKind::Jump,
            _ => continue,
        };

        let displacement =
            i32::from_le_bytes([bytes[i + 1], bytes[i + 2], bytes[i + 3], bytes[i + 4]]);

        let site = image.base + i as u64;
        // rel32 is relative to the address of the next instruction.
        let resolved = (site + REL32_INSTRUCTION_LEN).wrapping_add(displacement as i64 as u64);

        if resolved == target {
            out.push(Xref { from: site, kind });
        }
    }

    out
}

/// Finds every 4-byte-aligned occurrence of `address` as a literal value.
///
/// Used for globals and for addresses that are taken rather than called. Not
/// restricted to aligned positions, because x86 immediates are not aligned.
pub fn data_refs(image: &Image, address: u64) -> Vec<Xref> {
    let needle = (address as u32).to_le_bytes();
    let mut out = Vec::new();

    if image.bytes.len() < 4 {
        return out;
    }

    for i in 0..image.bytes.len() - 4 {
        if image.bytes[i..i + 4] == needle {
            out.push(Xref {
                from: image.base + i as u64,
                kind: RefKind::Data,
            });
        }
    }

    out
}

/// Walks backwards from `site` to find a plausible start for the enclosing
/// function.
///
/// MSVC pads between functions with `int3`, so the first padding run before the
/// site is a good guess. Returns `None` when no padding is found within
/// `max_back` bytes, which usually means the site is not really code.
pub fn enclosing_function(image: &Image, site: u64, max_back: u64) -> Option<u64> {
    let lowest = site.saturating_sub(max_back).max(image.base);
    let mut va = site;

    while va > lowest {
        let Some(window) = image.slice(va - 4, 4) else {
            return None;
        };

        if window == [0xCC, 0xCC, 0xCC, 0xCC] {
            return Some(va);
        }

        va -= 1;
    }

    None
}

/// Decodes a window around a reference site so it can be classified by eye.
pub fn context(image: &Image, site: u64, before: u64, count: usize) {
    let start = site.saturating_sub(before).max(image.base);
    let lines = disasm::decode(image, start, count);

    for line in &lines {
        let marker = if line.va == site { ">>" } else { "  " };
        let hex: Vec<String> = line.bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!(
            "{} 0x{:08X}  {:<24}  {}",
            marker,
            line.va,
            hex.join(" "),
            line.text
        );
    }
}
