//! Classifies the functions that reach a bag accessor.
//!
//! The sliding-window design keeps the game's 6-slot bag and swaps its contents
//! as the player scrolls. Code that reads one known slot is unaffected by that.
//! Code that *scans* the bag is: it only ever sees the six entries currently in
//! view, so "do I have the key?" answers no while the key sits outside the
//! window.
//!
//! Telling the two apart by hand across every call site is slow and easy to get
//! wrong, so each function is scored on signals that a scan leaves behind.

use std::collections::BTreeMap;

use iced_x86::{Instruction, Mnemonic, OpKind, Register};

use crate::disasm;
use crate::image::Image;
use crate::xref;

/// Slots the game allocates per bag.
const BAG_SLOTS: i64 = 6;
/// `Item` is 8 bytes, so the items array spans 6 * 8 = 0x30 bytes.
const ITEMS_ARRAY_BYTES: i64 = 0x30;
const ITEM_STRIDE: u32 = 8;

/// Offsets that sit immediately after the items array. Code touching these is
/// making an assumption about where the array ends.
const OFFSET_PERSONAL_ITEM: u32 = 0x34;
const OFFSET_EQUIPPED_INDEX: u32 = 0x3C;

/// How far back to search for the start of the enclosing function.
const MAX_FUNCTION_SEARCH: u64 = 0x2000;

/// Instructions decoded after a call site when scoring it.
///
/// The accessor returns the bag pointer in `eax`, and compiled code uses a
/// returned pointer close to the call. Scoring the whole enclosing function
/// instead would count every unrelated `+0x34` and `+0x3C` in it, which in a
/// 1800-instruction function is mostly noise.
///
/// The cost of the window is the opposite error: a function that stashes the
/// pointer and walks the bag hundreds of instructions later scores as a single
/// slot access. Those are why the whole-function score is still reported.
const CALL_SITE_WINDOW: usize = 48;

#[derive(Default)]
pub struct Signals {
    /// `cmp reg, 6` or `cmp reg, 5`: a loop bounded by the slot count.
    pub slot_count_compare: usize,
    /// Immediate `0x30`: the byte length of the items array.
    pub array_size_immediate: usize,
    /// Memory operand scaled by 8: indexing the items array by element.
    pub item_indexing: usize,
    /// Access at `+0x34`, the personal item.
    pub personal_item: usize,
    /// Access at `+0x3C`, the equipped index.
    pub equipped_index: usize,
    /// Backward branch: a loop of some kind.
    pub backward_branch: usize,
    pub instructions: usize,
}

impl Signals {
    /// Whether the function looks like it walks the whole bag.
    ///
    /// Deliberately generous: a false positive costs one manual read, a false
    /// negative ships a bug where an item silently stops being found.
    pub fn scans_bag(&self) -> bool {
        (self.slot_count_compare > 0 && self.backward_branch > 0)
            || self.array_size_immediate > 0
            || (self.item_indexing > 0 && self.backward_branch > 0)
    }

    /// Whether it depends on where the items array ends.
    pub fn assumes_layout(&self) -> bool {
        self.personal_item > 0 || self.equipped_index > 0
    }

    pub fn verdict(&self) -> &'static str {
        match (self.scans_bag(), self.assumes_layout()) {
            (true, true) => "SCANS + LAYOUT",
            (true, false) => "SCANS",
            (false, true) => "LAYOUT",
            (false, false) => "single slot",
        }
    }
}

fn inspect(instruction: &Instruction, signals: &mut Signals) {
    signals.instructions += 1;

    for operand in 0..instruction.op_count() {
        let kind = instruction.op_kind(operand);

        let is_immediate = matches!(
            kind,
            OpKind::Immediate8
                | OpKind::Immediate8_2nd
                | OpKind::Immediate16
                | OpKind::Immediate32
                | OpKind::Immediate8to16
                | OpKind::Immediate8to32
        );

        if is_immediate {
            let value = instruction.immediate(operand) as i64;

            if instruction.mnemonic() == Mnemonic::Cmp
                && (value == BAG_SLOTS || value == BAG_SLOTS - 1)
            {
                signals.slot_count_compare += 1;
            }

            if value == ITEMS_ARRAY_BYTES {
                signals.array_size_immediate += 1;
            }
        }
    }

    let uses_memory = instruction.memory_base() != Register::None
        || instruction.memory_index() != Register::None;

    if uses_memory {
        if instruction.memory_index_scale() == ITEM_STRIDE {
            signals.item_indexing += 1;
        }

        match instruction.memory_displacement32() {
            OFFSET_PERSONAL_ITEM => signals.personal_item += 1,
            OFFSET_EQUIPPED_INDEX => signals.equipped_index += 1,
            _ => {}
        }
    }
}

fn count_backward_branch(line: &disasm::Line, signals: &mut Signals) {
    let branch = line.instruction.near_branch_target();
    if branch != 0 && branch < line.va {
        signals.backward_branch += 1;
    }
}

/// Scores the instructions following one call site.
fn score(image: &Image, site: u64, window: usize, signals: &mut Signals) {
    // Skip the call itself.
    for line in disasm::decode(image, site, window).iter().skip(1) {
        inspect(&line.instruction, signals);
        count_backward_branch(line, signals);
    }
}

/// Scores every function that directly calls `target`.
pub fn report(image: &Image, target: u64) {
    let refs = xref::code_refs(image, target);

    // Several call sites usually live in the same function; score each once.
    let mut sites_by_function: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut orphans = Vec::new();

    for r in &refs {
        match xref::enclosing_function(image, r.from, MAX_FUNCTION_SEARCH) {
            Some(start) => sites_by_function.entry(start).or_default().push(r.from),
            None => orphans.push(r.from),
        }
    }

    println!(
        "{} call sites to 0x{:08X}, in {} distinct functions",
        refs.len(),
        target,
        sites_by_function.len()
    );
    println!();
    println!("Scored over {CALL_SITE_WINDOW} instructions following each call site.");
    println!("'fn' is the same score over the whole enclosing function, for context only:");
    println!("it counts unrelated struct offsets and is not a verdict.");
    println!();
    println!(
        "{:<12} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}  {:<16} fn",
        "function", "sites", "cmp6", "0x30", "idx8", "+34", "+3C", "verdict"
    );
    println!("{}", "-".repeat(88));

    let mut scanning = 0;
    let mut layout = 0;

    for (start, sites) in &sites_by_function {
        let mut signals = Signals::default();
        for site in sites {
            score(image, *site, CALL_SITE_WINDOW, &mut signals);
        }

        let mut whole = Signals::default();
        for line in disasm::function(image, *start) {
            inspect(&line.instruction, &mut whole);
            count_backward_branch(&line, &mut whole);
        }

        if signals.scans_bag() {
            scanning += 1;
        }
        if signals.assumes_layout() {
            layout += 1;
        }

        println!(
            "0x{:08X} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}  {:<16} {}",
            start,
            sites.len(),
            signals.slot_count_compare,
            signals.array_size_immediate,
            signals.item_indexing,
            signals.personal_item,
            signals.equipped_index,
            signals.verdict(),
            whole.verdict()
        );
    }

    if !orphans.is_empty() {
        println!();
        println!("{} sites with no identifiable enclosing function:", orphans.len());
        for site in &orphans {
            println!("  0x{:08X}", site);
        }
    }

    println!();
    println!("Functions that appear to scan the whole bag: {scanning}");
    println!("Functions that assume the layout after the items array: {layout}");
    println!();
    println!("Each scanning function needs a hook that consults the full store,");
    println!("otherwise items outside the visible window become invisible to it.");
}
