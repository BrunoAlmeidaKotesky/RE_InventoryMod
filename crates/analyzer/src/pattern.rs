//! Generic byte-pattern search with wildcards.
//!
//! Some questions are easiest to ask as "where does this instruction encoding
//! appear", without teaching the tool what the instruction means. A struct copy
//! of a known size, or a `lea` with a specific displacement, are both easier to
//! express as bytes than as a decoder rule.

use std::collections::BTreeMap;

use crate::image::Image;
use crate::xref;

/// How far back to search for the start of the enclosing function.
const MAX_FUNCTION_SEARCH: u64 = 0x2000;

/// A byte that must match exactly, or `None` for a wildcard.
pub type Pattern = Vec<Option<u8>>;

/// Parses a pattern like `8D ?? 60` or `8D??60`. `??` and `?` are wildcards.
pub fn parse(text: &str) -> Result<Pattern, String> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();

    if cleaned.len() % 2 != 0 {
        return Err(format!("'{text}' has an odd number of hex digits"));
    }

    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let chars: Vec<char> = cleaned.chars().collect();

    for pair in chars.chunks(2) {
        if pair[0] == '?' && pair[1] == '?' {
            out.push(None);
            continue;
        }

        let byte = u8::from_str_radix(&format!("{}{}", pair[0], pair[1]), 16)
            .map_err(|_| format!("'{}{}' is not a hex byte", pair[0], pair[1]))?;
        out.push(Some(byte));
    }

    if out.is_empty() {
        return Err("empty pattern".to_string());
    }

    Ok(out)
}

pub fn find(image: &Image, pattern: &Pattern) -> Vec<u64> {
    let bytes = &image.bytes;
    let mut out = Vec::new();

    if bytes.len() < pattern.len() {
        return out;
    }

    for i in 0..=bytes.len() - pattern.len() {
        let matches = pattern
            .iter()
            .enumerate()
            .all(|(j, expected)| expected.is_none_or(|b| bytes[i + j] == b));

        if matches {
            out.push(image.base + i as u64);
        }
    }

    out
}

/// Prints matches grouped by enclosing function, optionally restricted to an
/// address range.
pub fn report(image: &Image, pattern: &Pattern, range: Option<(u64, u64)>) {
    let all = find(image, pattern);

    let hits: Vec<u64> = match range {
        Some((low, high)) => all.iter().copied().filter(|a| (low..high).contains(a)).collect(),
        None => all.clone(),
    };

    let mut by_function: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut orphans = 0;

    for &hit in &hits {
        match xref::enclosing_function(image, hit, MAX_FUNCTION_SEARCH) {
            Some(start) => by_function.entry(start).or_default().push(hit),
            None => orphans += 1,
        }
    }

    match range {
        Some((low, high)) => println!(
            "{} matches in 0x{:08X}-0x{:08X} ({} in the whole section), in {} functions",
            hits.len(),
            low,
            high,
            all.len(),
            by_function.len()
        ),
        None => println!("{} matches, in {} functions", hits.len(), by_function.len()),
    }

    println!();

    for (function, sites) in &by_function {
        let listed: Vec<String> = sites.iter().take(8).map(|a| format!("0x{:08X}", a)).collect();
        let suffix = if sites.len() > 8 {
            format!(" +{}", sites.len() - 8)
        } else {
            String::new()
        };
        println!(
            "0x{:08X}  {:>3}  {}{}",
            function,
            sites.len(),
            listed.join(" "),
            suffix
        );
    }

    if orphans > 0 {
        println!();
        println!("{orphans} matches with no identifiable enclosing function");
    }
}
