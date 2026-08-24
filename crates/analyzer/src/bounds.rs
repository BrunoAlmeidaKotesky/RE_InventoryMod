//! Finds every site that indexes a fixed-size array with a runtime index.
//!
//! The game uses `tsl::array`, whose `operator[]` asserts the index is in
//! range. The compiler inlines that assert everywhere, leaving a very
//! recognisable shape:
//!
//! ```asm
//! cmp  esi, 6                  ; index < capacity?
//! jb   short ok
//! push <message string>
//! push 50                      ; line number in tsl/array.h
//! push <file string>
//! call assert
//! ok:
//! ```
//!
//! Searching for that shape with a capacity of 6 finds the places that walk or
//! index a bag's item array, including the ones reached from an accessor whose
//! result was stashed far earlier. Cross-referencing the accessors alone misses
//! those.
//!
//! It also finds any *other* six-element array in the game, so each hit still
//! needs a look. Six is an unusual capacity, and the false positives are cheap
//! to dismiss.

use std::collections::BTreeMap;

use iced_x86::{Mnemonic, Register};

use crate::image::Image;
use crate::xref;

/// `push imm32`, used to push the assert's file string.
const OP_PUSH_IMM32: u8 = 0x68;

/// `cmp r32, imm8` is `83 /7 ib`; ModRM for a register operand is `0xF8 | reg`.
const OP_CMP_R32_IMM8: u8 = 0x83;
const MODRM_CMP_REG_MIN: u8 = 0xF8;
const MODRM_CMP_REG_MAX: u8 = 0xFF;

/// How far back from the assert to look for the bound check.
const LOOKBACK: usize = 24;

/// How far back to search for the start of the enclosing function.
const MAX_FUNCTION_SEARCH: u64 = 0x2000;

/// `.rdata` address of "D:\BH0_PC_KANTAIJI\Game\lib\tsl/array.h".
pub const DEFAULT_ARRAY_ASSERT_STRING: u64 = 0x00CB_57D0;

#[derive(Debug, Clone)]
pub struct Site {
    /// Address of the `cmp reg, N` that guards the assert.
    pub check: u64,
}

/// Finds bound checks against `capacity` that guard the array assert.
pub fn find(image: &Image, capacity: u8, assert_string: u64) -> Vec<Site> {
    let needle = (assert_string as u32).to_le_bytes();
    let bytes = &image.bytes;
    let mut out = Vec::new();

    if bytes.len() < 5 {
        return out;
    }

    for i in 0..bytes.len() - 5 {
        if bytes[i] != OP_PUSH_IMM32 || bytes[i + 1..i + 5] != needle {
            continue;
        }

        // Walk back looking for `cmp reg, capacity`. Scanning backwards over
        // variable-length instructions cannot be exact, but this pattern is
        // three bytes and immediately precedes the assert.
        let start = i.saturating_sub(LOOKBACK);
        let mut found = None;

        for j in (start..i).rev() {
            if bytes[j] == OP_CMP_R32_IMM8
                && (MODRM_CMP_REG_MIN..=MODRM_CMP_REG_MAX).contains(&bytes[j + 1])
                && bytes[j + 2] == capacity
            {
                found = Some(image.base + j as u64);
                break;
            }
        }

        if let Some(check) = found {
            out.push(Site { check });
        }
    }

    out
}

/// Instructions decoded forward from a bound check when deciding whether it
/// sits inside a loop.
const LOOP_WINDOW: usize = 64;

/// How the index reaching a bound check is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The index register is advanced and control branches back: the code walks
    /// the array. Under a sliding window this only ever sees the visible slots.
    Iterates,
    /// The index is not advanced near the check: one slot, chosen elsewhere.
    /// A sliding window does not change what this reads.
    SingleSlot,
}

/// Decides whether a bound check guards a loop.
///
/// Looks for the two halves of an iteration around the check: the index
/// register being advanced, and a branch going backwards to at or before the
/// check. Requiring both keeps a plain `if (index < 6)` out of the results.
fn access_at(image: &Image, check: u64) -> Access {
    let lines = crate::disasm::decode(image, check, LOOP_WINDOW);

    let Some(first) = lines.first() else {
        return Access::SingleSlot;
    };

    let index = first.instruction.op0_register();
    if index == Register::None {
        return Access::SingleSlot;
    }

    let mut advances_index = false;
    let mut branches_backward = false;

    for line in lines.iter().skip(1) {
        let instruction = &line.instruction;

        // `inc reg` or `add reg, imm`, on the register that was bound-checked.
        let advances = matches!(instruction.mnemonic(), Mnemonic::Inc | Mnemonic::Add)
            && instruction.op0_register() == index;

        if advances {
            advances_index = true;
        }

        let target = instruction.near_branch_target();
        if target != 0 && target <= check && target >= image.base {
            branches_backward = true;
        }

        // A second check on the same register means a new loop; stop here so
        // the two are not conflated.
        if line.va != check
            && instruction.mnemonic() == Mnemonic::Cmp
            && instruction.op0_register() == index
            && branches_backward
        {
            break;
        }
    }

    if advances_index && branches_backward {
        Access::Iterates
    } else {
        Access::SingleSlot
    }
}

pub fn report(image: &Image, capacity: u8, assert_string: u64) {
    let sites = find(image, capacity, assert_string);

    let mut by_function: BTreeMap<u64, Vec<&Site>> = BTreeMap::new();
    let mut orphans = Vec::new();

    for site in &sites {
        match xref::enclosing_function(image, site.check, MAX_FUNCTION_SEARCH) {
            Some(start) => by_function.entry(start).or_default().push(site),
            None => orphans.push(site),
        }
    }

    println!(
        "{} bound checks against {} guarding the array assert, in {} functions",
        sites.len(),
        capacity,
        by_function.len()
    );
    println!();
    println!(
        "{:<12} {:>7} {:>10} {:>7}  iterating check sites",
        "function", "checks", "iterating", "single"
    );
    println!("{}", "-".repeat(92));

    let mut iterating_functions = 0;
    let mut iterating_sites = 0;

    for (function, hits) in &by_function {
        let mut iterating = Vec::new();
        let mut single = 0;

        for site in hits {
            match access_at(image, site.check) {
                Access::Iterates => iterating.push(site.check),
                Access::SingleSlot => single += 1,
            }
        }

        if !iterating.is_empty() {
            iterating_functions += 1;
            iterating_sites += iterating.len();
        }

        let listed: Vec<String> = iterating
            .iter()
            .take(4)
            .map(|va| format!("0x{:08X}", va))
            .collect();
        let suffix = if iterating.len() > 4 {
            format!(" +{}", iterating.len() - 4)
        } else {
            String::new()
        };

        println!(
            "0x{:08X}  {:>7} {:>10} {:>7}  {}{}",
            function,
            hits.len(),
            iterating.len(),
            single,
            listed.join(" "),
            suffix
        );
    }

    if !orphans.is_empty() {
        println!();
        println!("{} checks with no identifiable enclosing function", orphans.len());
    }

    println!();
    println!("{iterating_sites} of {} checks iterate, across {iterating_functions} functions.", sites.len());
    println!();
    println!("A single-slot access keeps working under a sliding window; the slot it");
    println!("reads is whichever one the window is showing. An iterating access does");
    println!("not: it walks only the visible slots and misses the rest of the store.");
    println!();
    println!("Not all of these touch a bag. Any six-element tsl::array lands here, so");
    println!("each iterating function still has to be read before it is called a bag site.");
}
