//! Disassembly helpers built on iced-x86.

use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic};

use crate::image::Image;

/// 32-bit code.
const BITNESS: u32 = 32;

/// Ceiling for `function`, so a bad start address cannot run away.
const MAX_FUNCTION_INSTRUCTIONS: usize = 8192;
/// Ceiling on how far `extent` searches for the padding that ends a function.
const MAX_FUNCTION_BYTES: usize = 0x4000;

/// MSVC fills the gap between functions with this.
const INT3: u8 = 0xCC;
/// Consecutive `int3` bytes that count as padding rather than a real
/// instruction. Three is enough to be unambiguous in compiled code.
const PADDING_RUN: usize = 3;

pub struct Line {
    pub va: u64,
    pub bytes: Vec<u8>,
    pub text: String,
    pub instruction: Instruction,
}

fn formatter() -> IntelFormatter {
    let mut f = IntelFormatter::new();
    f.options_mut().set_hex_prefix("0x");
    f.options_mut().set_hex_suffix("");
    f.options_mut().set_uppercase_hex(true);
    f.options_mut().set_space_after_operand_separator(true);
    f
}

/// Decodes `count` instructions starting at `va`.
pub fn decode(image: &Image, va: u64, count: usize) -> Vec<Line> {
    let Some(bytes) = image.from(va) else {
        return Vec::new();
    };

    let mut decoder = Decoder::with_ip(BITNESS, bytes, va, DecoderOptions::NONE);
    let mut formatter = formatter();
    let mut out = Vec::with_capacity(count);

    let mut instruction = Instruction::default();
    while out.len() < count && decoder.can_decode() {
        let start = decoder.ip();
        decoder.decode_out(&mut instruction);

        let mut text = String::new();
        formatter.format(&instruction, &mut text);

        let offset = (start - va) as usize;
        let raw = bytes[offset..offset + instruction.len()].to_vec();

        out.push(Line {
            va: start,
            bytes: raw,
            text,
            instruction,
        });
    }

    out
}

/// Finds where the function starting at `va` ends.
///
/// MSVC aligns functions and fills the gap with `int3`, so the next padding run
/// is the end. This is far more reliable than following control flow: a
/// function with a jump table, or one whose first `ret` is an early exit,
/// defeats any "stop at the first return" rule, and stopping early silently
/// hides everything after that point.
///
/// Returns `None` when no padding is found within `MAX_FUNCTION_BYTES`.
pub fn extent(image: &Image, va: u64) -> Option<u64> {
    let bytes = image.from(va)?;
    let limit = MAX_FUNCTION_BYTES.min(bytes.len());

    let mut run = 0usize;
    for (i, &b) in bytes[..limit].iter().enumerate() {
        if b == INT3 {
            run += 1;
            if run >= PADDING_RUN {
                // The padding starts where the run started.
                return Some(va + (i + 1 - run) as u64);
            }
        } else {
            run = 0;
        }
    }

    None
}

/// Decodes a whole function: from `va` to the padding that follows it.
///
/// Data embedded between basic blocks (jump tables, in particular) decodes as
/// nonsense instructions. That is accepted here: the callers of this function
/// count signals across the body, and a handful of bogus instructions is far
/// less damaging than truncating the body at the first `ret`.
pub fn function(image: &Image, va: u64) -> Vec<Line> {
    match extent(image, va) {
        Some(end) => {
            let count = ((end - va) as usize).min(MAX_FUNCTION_INSTRUCTIONS);
            decode(image, va, count)
                .into_iter()
                .take_while(|line| line.va < end)
                .collect()
        }
        None => decode_until_return(image, va),
    }
}

/// Fallback for functions with no padding after them.
fn decode_until_return(image: &Image, va: u64) -> Vec<Line> {
    let mut furthest_branch = va;
    let mut out = Vec::new();

    for line in decode(image, va, MAX_FUNCTION_INSTRUCTIONS) {
        let mnemonic = line.instruction.mnemonic();

        // A `ret` before the furthest known branch target is one exit of
        // several, not the end of the function.
        if is_branch(mnemonic) && line.instruction.near_branch_target() > furthest_branch {
            furthest_branch = line.instruction.near_branch_target();
        }

        let at = line.va;
        let terminates = matches!(mnemonic, Mnemonic::Ret | Mnemonic::Retf);
        out.push(line);

        if terminates && at >= furthest_branch {
            break;
        }
    }

    out
}

fn is_branch(m: Mnemonic) -> bool {
    matches!(
        m,
        Mnemonic::Jmp
            | Mnemonic::Je
            | Mnemonic::Jne
            | Mnemonic::Jb
            | Mnemonic::Jbe
            | Mnemonic::Ja
            | Mnemonic::Jae
            | Mnemonic::Jl
            | Mnemonic::Jle
            | Mnemonic::Jg
            | Mnemonic::Jge
            | Mnemonic::Js
            | Mnemonic::Jns
            | Mnemonic::Jo
            | Mnemonic::Jno
            | Mnemonic::Jp
            | Mnemonic::Jnp
            | Mnemonic::Jcxz
            | Mnemonic::Jecxz
    )
}

pub fn print(lines: &[Line]) {
    for line in lines {
        let hex: Vec<String> = line.bytes.iter().map(|b| format!("{:02X}", b)).collect();
        println!("0x{:08X}  {:<24}  {}", line.va, hex.join(" "), line.text);
    }
}
