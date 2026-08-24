//! Finds memory operands using a given displacement.
//!
//! Answers "how much code would have to change if this field moved". The fields
//! after a fixed-size array move when the array grows, and every access to them
//! is encoded with the old offset as a literal displacement.
//!
//! Unlike the byte-pattern search, this decodes, so it finds the offset in
//! every addressing form rather than in the handful of encodings someone
//! thought to write out.

use std::collections::BTreeMap;

use iced_x86::Register;

use crate::disasm;
use crate::image::Image;
use crate::xref;

const MAX_FUNCTION_SEARCH: u64 = 0x2000;

/// Instructions decoded per linear pass. A range is decoded straight through,
/// so it should start on a known instruction boundary, ideally a function start.
const MAX_INSTRUCTIONS: usize = 200_000;

#[derive(Debug, Clone)]
pub struct Use {
    pub va: u64,
    pub text: String,
}

pub fn find(image: &Image, displacement: u32, low: u64, high: u64) -> Vec<Use> {
    let mut out = Vec::new();

    for line in disasm::decode(image, low, MAX_INSTRUCTIONS) {
        if line.va >= high {
            break;
        }

        let instruction = &line.instruction;
        let base = instruction.memory_base();

        // A displacement with no base register is an absolute address, not a
        // field offset, so it is not interesting here.
        if base == Register::None {
            continue;
        }

        if instruction.memory_displacement32() == displacement {
            out.push(Use {
                va: line.va,
                text: line.text.clone(),
            });
        }
    }

    out
}

pub fn report(image: &Image, displacement: u32, low: u64, high: u64) {
    let uses = find(image, displacement, low, high);

    let mut by_function: BTreeMap<u64, Vec<&Use>> = BTreeMap::new();
    let mut orphans = 0;

    for use_site in &uses {
        match xref::enclosing_function(image, use_site.va, MAX_FUNCTION_SEARCH) {
            Some(start) => by_function.entry(start).or_default().push(use_site),
            None => orphans += 1,
        }
    }

    println!(
        "{} memory operands with displacement 0x{:X} in 0x{:08X}-0x{:08X}, in {} functions",
        uses.len(),
        displacement,
        low,
        high,
        by_function.len()
    );
    println!();

    for (function, sites) in &by_function {
        println!("0x{:08X}  {} use(s)", function, sites.len());
        for site in sites {
            println!("    0x{:08X}  {}", site.va, site.text);
        }
    }

    if orphans > 0 {
        println!();
        println!("{orphans} uses with no identifiable enclosing function");
    }

    println!();
    println!("Linear decoding does not know where data is embedded in code, so a");
    println!("few of these may be decoded from something that is not an instruction.");
}
