//! The item list behind the window, and the rules that keep it valid.
//!
//! # Why the rules exist
//!
//! Some items occupy two slots. The game represents that as the item itself
//! followed by a filler entry, and it addresses the inventory as rows of two.
//! A two-slot item may therefore only start at an even index; starting one at an
//! odd index splits it across two rows, which the game draws and handles wrongly.
//!
//! Every operation here either preserves that or repairs it. The game itself
//! leaves the bag inconsistent partway through an exchange, so repair is not an
//! error path — it is a normal step.

use crate::game::inventory::{is_two_slot, Item, BAG_SIZE, SLOT_TWO_FILLER};

/// One logical entry, before it is laid out into slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// Occupies one slot.
    Single(Item),
    /// Occupies two: the item, then the filler.
    Double(Item),
}

impl Entry {
    fn width(self) -> usize {
        match self {
            Entry::Single(_) => 1,
            Entry::Double(_) => 2,
        }
    }
}

/// The mod's item storage for one character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slots {
    items: Vec<Item>,
}

impl Slots {
    /// Creates an empty store.
    ///
    /// The capacity is forced even and at least the game's own size. An odd
    /// capacity would leave a trailing slot that can never hold the second half
    /// of a two-slot item, and every scroll position would be a special case.
    pub fn new(capacity: usize) -> Slots {
        let capacity = capacity.max(BAG_SIZE).next_multiple_of(2);
        Slots {
            items: vec![Item::EMPTY; capacity],
        }
    }

    pub fn capacity(&self) -> usize {
        self.items.len()
    }

    pub fn as_slice(&self) -> &[Item] {
        &self.items
    }

    pub fn get(&self, index: usize) -> Option<Item> {
        self.items.get(index).copied()
    }

    pub fn set(&mut self, index: usize, item: Item) -> bool {
        match self.items.get_mut(index) {
            Some(slot) => {
                *slot = item;
                true
            }
            None => false,
        }
    }

    /// Copies `count` slots starting at `start`, padding with empties if the
    /// range runs past the end.
    #[cfg(test)]
    pub fn view(&self, start: usize, count: usize) -> Vec<Item> {
        (start..start + count)
            .map(|i| self.items.get(i).copied().unwrap_or(Item::EMPTY))
            .collect()
    }

    /// Index of the first empty slot, or `None` when full.
    pub fn first_empty(&self) -> Option<usize> {
        self.items.iter().position(|item| item.is_empty())
    }

    pub fn count_empty(&self) -> usize {
        self.items.iter().filter(|item| item.is_empty()).count()
    }

    /// Index of the first slot holding `id`.
    ///
    /// The filler is never reported: it is an implementation detail of two-slot
    /// items, not something the player holds.
    pub fn find(&self, id: i32) -> Option<usize> {
        if id == SLOT_TWO_FILLER || id == Item::EMPTY.id {
            return None;
        }
        self.items.iter().position(|item| item.id == id)
    }

    pub fn total_count(&self, id: i32) -> i64 {
        self.items
            .iter()
            .filter(|item| item.id == id)
            .map(|item| i64::from(item.count))
            .sum()
    }

    /// Whether the two-slot pairing holds everywhere.
    ///
    /// This is about pairing only, not placement: a correctly paired two-slot
    /// item sitting at an odd index is not broken, it is misaligned.
    pub fn is_broken(&self) -> bool {
        for (i, item) in self.items.iter().enumerate() {
            if is_two_slot(item.id) {
                // A two-slot item must be followed by the filler, and cannot be
                // the last slot.
                match self.items.get(i + 1) {
                    Some(next) if next.id == SLOT_TWO_FILLER => {}
                    _ => return true,
                }
            }

            if item.id == SLOT_TWO_FILLER {
                // A filler must follow a two-slot item, and cannot be first.
                match i.checked_sub(1).and_then(|prev| self.items.get(prev)) {
                    Some(previous) if is_two_slot(previous.id) => {}
                    _ => return true,
                }
            }
        }

        false
    }

    /// Whether every two-slot item starts on an even index.
    pub fn is_aligned(&self) -> bool {
        !self
            .items
            .iter()
            .enumerate()
            .any(|(i, item)| is_two_slot(item.id) && !i.is_multiple_of(2))
    }

    /// Whether the used slots are packed towards the front, with holes only
    /// where alignment forces one.
    pub fn is_packed(&self) -> bool {
        let mut seen_empty_at = None;

        for (i, item) in self.items.iter().enumerate() {
            if item.is_empty() {
                if seen_empty_at.is_none() {
                    seen_empty_at = Some(i);
                }
                continue;
            }

            // A non-empty slot after an empty one is only allowed when the empty
            // slot was padding needed to put a two-slot item on an even index.
            if let Some(empty) = seen_empty_at {
                let forced = !empty.is_multiple_of(2) && is_two_slot(item.id) && i == empty + 1;
                if !forced {
                    return false;
                }
                seen_empty_at = None;
            }
        }

        true
    }

    pub fn is_valid(&self) -> bool {
        !self.is_broken() && self.is_aligned() && self.is_packed()
    }

    /// Rebuilds the layout so that it satisfies every rule.
    ///
    /// Returns whatever did not fit, which can only happen if the capacity
    /// shrank since the contents were stored. Losing a player's items silently
    /// is not an acceptable failure mode, so they are handed back rather than
    /// dropped.
    pub fn repair(&mut self) -> Vec<Item> {
        let entries = self.extract_entries();
        self.lay_out(entries)
    }

    /// Reduces the slots to logical entries, discarding the encoding.
    ///
    /// Orphan fillers are dropped: a filler with no two-slot item in front of it
    /// represents nothing the player owns. A two-slot item missing its filler
    /// keeps its second slot, which `lay_out` writes back.
    fn extract_entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut i = 0;

        while i < self.items.len() {
            let item = self.items[i];

            if item.is_empty() || item.id == SLOT_TWO_FILLER {
                i += 1;
                continue;
            }

            if is_two_slot(item.id) {
                entries.push(Entry::Double(item));
                // Skip the filler if it is there; if it is not, the entry is
                // still a double and gets a fresh one on the way out.
                let skip = matches!(self.items.get(i + 1), Some(next) if next.id == SLOT_TWO_FILLER);
                i += if skip { 2 } else { 1 };
                continue;
            }

            entries.push(Entry::Single(item));
            i += 1;
        }

        entries
    }

    /// Writes entries back into slots, keeping every two-slot item on an even
    /// index.
    ///
    /// When a double would land on an odd index, a single is pulled forward to
    /// absorb the parity. That reorders the inventory, which the player can see,
    /// but the alternative is a hole, and the game's own sort reorders too.
    /// A hole is only left when there is no single available to move.
    fn lay_out(&mut self, entries: Vec<Entry>) -> Vec<Item> {
        let capacity = self.items.len();
        let mut out = vec![Item::EMPTY; capacity];
        let mut pending: Vec<Entry> = entries;
        let mut cursor = 0usize;

        while !pending.is_empty() {
            let Some(position) = choose_next(&pending, cursor) else {
                // Nothing placeable here: leave the slot empty and move on so
                // the next index has the other parity.
                if cursor >= capacity {
                    break;
                }
                cursor += 1;
                continue;
            };

            let entry = pending.remove(position);
            if cursor + entry.width() > capacity {
                // Does not fit. Put it back and stop; the rest is overflow.
                pending.insert(position, entry);
                break;
            }

            match entry {
                Entry::Single(item) => {
                    out[cursor] = item;
                    cursor += 1;
                }
                Entry::Double(item) => {
                    out[cursor] = item;
                    out[cursor + 1] = Item::filler();
                    cursor += 2;
                }
            }
        }

        self.items = out;

        pending
            .into_iter()
            .map(|entry| match entry {
                Entry::Single(item) | Entry::Double(item) => item,
            })
            .collect()
    }
}

/// Picks which pending entry to place at `cursor`.
///
/// On an even index anything fits, so the first entry keeps the original order.
/// On an odd index only a single fits, so the first single is pulled forward.
fn choose_next(pending: &[Entry], cursor: usize) -> Option<usize> {
    if cursor.is_multiple_of(2) {
        return (!pending.is_empty()).then_some(0);
    }

    pending
        .iter()
        .position(|entry| matches!(entry, Entry::Single(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two-slot item id, from the game's table.
    const SHOTGUN: i32 = 6;
    const HANDGUN_ROUNDS: i32 = 20;
    const HERB: i32 = 30;

    fn single(id: i32) -> Item {
        Item { id, count: 1 }
    }

    fn slots_from(ids: &[i32]) -> Slots {
        let capacity = ids.len().max(BAG_SIZE).next_multiple_of(2);
        let mut slots = Slots::new(capacity);
        for (i, &id) in ids.iter().enumerate() {
            let item = if id == 0 { Item::EMPTY } else { single(id) };
            slots.set(i, item);
        }
        slots
    }

    fn ids(slots: &Slots) -> Vec<i32> {
        slots.as_slice().iter().map(|item| item.id).collect()
    }

    #[test]
    fn capacity_is_forced_even_and_at_least_the_game_size() {
        assert_eq!(Slots::new(0).capacity(), BAG_SIZE);
        assert_eq!(Slots::new(7).capacity(), 8);
        assert_eq!(Slots::new(8).capacity(), 8);
    }

    #[test]
    fn a_paired_two_slot_item_is_not_broken() {
        let slots = slots_from(&[SHOTGUN, SLOT_TWO_FILLER, HERB, 0, 0, 0]);
        assert!(!slots.is_broken());
        assert!(slots.is_valid());
    }

    #[test]
    fn a_two_slot_item_without_its_filler_is_broken() {
        let slots = slots_from(&[SHOTGUN, HERB, 0, 0, 0, 0]);
        assert!(slots.is_broken());
    }

    #[test]
    fn a_two_slot_item_in_the_last_slot_is_broken() {
        let slots = slots_from(&[HERB, HERB, HERB, HERB, HERB, SHOTGUN]);
        assert!(slots.is_broken());
    }

    #[test]
    fn an_orphan_filler_is_broken() {
        let slots = slots_from(&[HERB, SLOT_TWO_FILLER, 0, 0, 0, 0]);
        assert!(slots.is_broken());
    }

    #[test]
    fn a_leading_filler_is_broken() {
        let slots = slots_from(&[SLOT_TWO_FILLER, HERB, 0, 0, 0, 0]);
        assert!(slots.is_broken());
    }

    #[test]
    fn a_two_slot_item_on_an_odd_index_is_paired_but_misaligned() {
        let slots = slots_from(&[HERB, SHOTGUN, SLOT_TWO_FILLER, 0, 0, 0]);
        assert!(!slots.is_broken());
        assert!(!slots.is_aligned());
        assert!(!slots.is_valid());
    }

    #[test]
    fn repair_realigns_by_pulling_a_single_forward() {
        let mut slots = slots_from(&[HERB, SHOTGUN, SLOT_TWO_FILLER, HANDGUN_ROUNDS, 0, 0]);
        let overflow = slots.repair();

        assert!(overflow.is_empty());
        assert!(slots.is_valid());
        // The two singles absorb the parity so the shotgun lands on an even index.
        assert_eq!(
            ids(&slots),
            vec![HERB, HANDGUN_ROUNDS, SHOTGUN, SLOT_TWO_FILLER, 0, 0]
        );
    }

    #[test]
    fn repair_leaves_a_hole_when_no_single_can_absorb_the_parity() {
        // One single, then two doubles: after the single the cursor is odd and
        // nothing else can fill that slot.
        let mut slots = slots_from(&[
            HERB,
            SHOTGUN,
            SLOT_TWO_FILLER,
            SHOTGUN,
            SLOT_TWO_FILLER,
            0,
        ]);
        let overflow = slots.repair();

        assert!(overflow.is_empty());
        assert!(slots.is_aligned());
        assert!(!slots.is_broken());
        assert_eq!(
            ids(&slots),
            vec![HERB, 0, SHOTGUN, SLOT_TWO_FILLER, SHOTGUN, SLOT_TWO_FILLER]
        );
    }

    #[test]
    fn repair_drops_an_orphan_filler_and_keeps_the_item() {
        let mut slots = slots_from(&[HERB, SLOT_TWO_FILLER, HANDGUN_ROUNDS, 0, 0, 0]);
        slots.repair();

        assert!(slots.is_valid());
        assert_eq!(ids(&slots), vec![HERB, HANDGUN_ROUNDS, 0, 0, 0, 0]);
    }

    #[test]
    fn repair_restores_a_missing_filler() {
        let mut slots = slots_from(&[SHOTGUN, HANDGUN_ROUNDS, 0, 0, 0, 0]);
        slots.repair();

        assert!(slots.is_valid());
        assert_eq!(
            ids(&slots),
            vec![SHOTGUN, SLOT_TWO_FILLER, HANDGUN_ROUNDS, 0, 0, 0]
        );
    }

    #[test]
    fn repair_packs_gaps_towards_the_front() {
        let mut slots = slots_from(&[0, HERB, 0, HANDGUN_ROUNDS, 0, 0]);
        slots.repair();

        assert!(slots.is_valid());
        assert_eq!(ids(&slots), vec![HERB, HANDGUN_ROUNDS, 0, 0, 0, 0]);
    }

    #[test]
    fn repair_hands_back_what_does_not_fit() {
        let mut slots = Slots::new(BAG_SIZE);
        for i in 0..BAG_SIZE {
            slots.set(i, single(HERB));
        }
        // Force an entry that cannot be placed: a double needs two slots and
        // every slot is taken.
        let mut entries: Vec<Item> = slots.as_slice().to_vec();
        entries.push(single(SHOTGUN));

        let mut crowded = Slots::new(BAG_SIZE);
        for (i, item) in entries.iter().take(BAG_SIZE).enumerate() {
            crowded.set(i, *item);
        }
        // Replace the last single with a two-slot item that cannot be paired.
        crowded.set(BAG_SIZE - 1, single(SHOTGUN));

        let overflow = crowded.repair();

        assert!(crowded.is_valid());
        assert_eq!(overflow, vec![single(SHOTGUN)]);
    }

    #[test]
    fn repair_is_idempotent() {
        let mut slots = slots_from(&[HERB, SHOTGUN, SLOT_TWO_FILLER, 0, HANDGUN_ROUNDS, 0]);
        slots.repair();
        let once = slots.clone();
        slots.repair();

        assert_eq!(slots, once);
    }

    #[test]
    fn repair_never_loses_an_item_that_fits() {
        let mut slots = slots_from(&[0, SHOTGUN, SLOT_TWO_FILLER, HERB, 0, HANDGUN_ROUNDS]);
        let before: Vec<i32> = ids(&slots)
            .into_iter()
            .filter(|&id| id != 0 && id != SLOT_TWO_FILLER)
            .collect();

        let overflow = slots.repair();
        assert!(overflow.is_empty());

        let mut after: Vec<i32> = ids(&slots)
            .into_iter()
            .filter(|&id| id != 0 && id != SLOT_TWO_FILLER)
            .collect();

        let mut before_sorted = before;
        before_sorted.sort_unstable();
        after.sort_unstable();
        assert_eq!(before_sorted, after);
    }

    #[test]
    fn find_ignores_the_filler() {
        let slots = slots_from(&[SHOTGUN, SLOT_TWO_FILLER, HERB, 0, 0, 0]);

        assert_eq!(slots.find(SHOTGUN), Some(0));
        assert_eq!(slots.find(HERB), Some(2));
        assert_eq!(slots.find(SLOT_TWO_FILLER), None);
    }

    #[test]
    fn empty_slot_queries_span_the_whole_store() {
        let mut slots = Slots::new(12);
        slots.set(0, single(HERB));

        assert_eq!(slots.capacity(), 12);
        assert_eq!(slots.count_empty(), 11);
        assert_eq!(slots.first_empty(), Some(1));
    }

    #[test]
    fn a_view_past_the_end_is_padded_with_empties() {
        let mut slots = Slots::new(BAG_SIZE);
        slots.set(5, single(HERB));

        let view = slots.view(4, BAG_SIZE);
        assert_eq!(view.len(), BAG_SIZE);
        assert_eq!(view[0], Item::EMPTY);
        assert_eq!(view[1], single(HERB));
        assert!(view[2..].iter().all(|item| item.is_empty()));
    }
}
