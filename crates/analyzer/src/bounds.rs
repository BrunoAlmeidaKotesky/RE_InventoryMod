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
    /// Address of the `cmp reg, N`.
    pub check: u64,
    /// Address of the assert's `push`.
    pub assert: u64,
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
            out.push(Site {
                check,
                assert: image.base + i as u64,
            });
        }
    }

    out
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

    for (function, hits) in &by_function {
        let addresses: Vec<String> = hits.iter().map(|s| format!("0x{:08X}", s.check)).collect();
        println!(
            "0x{:08X}  {:>2} check(s)  {}",
            function,
            hits.len(),
            addresses.join(" ")
        );
    }

    if !orphans.is_empty() {
        println!();
        println!("{} checks with no identifiable enclosing function", orphans.len());
    }

    println!();
    println!("Not all of these touch a bag: any six-element tsl::array lands here.");
    println!("Each function still has to be read before it is called a bag site.");
}
