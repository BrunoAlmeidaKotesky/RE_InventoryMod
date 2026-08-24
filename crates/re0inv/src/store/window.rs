//! The six-slot window the game sees.
//!
//! The game reads and writes a `Bag` and has no idea anything else exists. The
//! window copies six slots out of the store into that bag before the game looks,
//! and copies whatever the game left there back afterwards.
//!
//! The window position is always even. The inventory is drawn and navigated as
//! rows of two, so an odd position would put the two halves of every row in
//! different rows, and would let a two-slot item straddle the window edge in a
//! way the game cannot draw.

use crate::game::inventory::{Bag, Item, BAG_SIZE};
use crate::store::slots::Slots;

pub struct Window {
    store: Slots,
    /// First store index the game can see. Always even.
    position: usize,
}

impl Window {
    pub fn new(capacity: usize) -> Window {
        Window {
            store: Slots::new(capacity),
            position: 0,
        }
    }

    pub fn store(&self) -> &Slots {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Slots {
        &mut self.store
    }

    pub fn position(&self) -> usize {
        self.position
    }

    /// Highest position that still shows a full window.
    ///
    /// Scrolling past this would show empty space beyond the end of the store
    /// rather than more items.
    pub fn max_position(&self) -> usize {
        self.store.capacity().saturating_sub(BAG_SIZE)
    }

    /// Whether there is anything to scroll to at all.
    pub fn is_scrollable(&self) -> bool {
        self.store.capacity() > BAG_SIZE
    }

    /// Moves the window by `rows` rows of two, clamped to the store.
    ///
    /// Returns whether the position actually changed, which is what decides
    /// between playing the movement sound and the rejection sound.
    pub fn scroll_rows(&mut self, rows: i32) -> bool {
        let delta = rows.saturating_mul(2);
        let target = (self.position as i64) + i64::from(delta);
        let clamped = target.clamp(0, self.max_position() as i64) as usize;

        // Clamping cannot break the invariant as long as the bounds are even,
        // and they are: position starts at zero and max_position is capacity
        // minus six, both even.
        let changed = clamped != self.position;
        self.position = clamped;
        changed
    }

    /// Jumps so that `index` is visible, keeping the window even-aligned.
    pub fn reveal(&mut self, index: usize) {
        if index < self.position {
            self.position = index & !1;
            return;
        }

        if index >= self.position + BAG_SIZE {
            let first = index + 1 - BAG_SIZE;
            self.position = (first.next_multiple_of(2)).min(self.max_position());
        }
    }

    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Copies the visible slots into the bag the game reads.
    ///
    /// Only the item slots are touched. The personal item and the equipped
    /// index belong to the bag itself, not to the store, and overwriting them
    /// here would discard state the game owns.
    pub fn write_into(&self, bag: &mut Bag) {
        let visible = self.store.view(self.position, BAG_SIZE);
        for (slot, item) in bag.items.iter_mut().zip(visible) {
            *slot = item;
        }
    }

    /// Copies whatever the game left in the bag back into the store.
    pub fn read_from(&mut self, bag: &Bag) {
        self.store.write_back(self.position, &bag.items);
    }

    /// Store index currently shown in visible slot `slot`.
    pub fn store_index(&self, slot: usize) -> Option<usize> {
        (slot < BAG_SIZE).then_some(self.position + slot)
    }

    /// Visible slot showing store index `index`, if it is in view.
    pub fn visible_slot(&self, index: usize) -> Option<usize> {
        index
            .checked_sub(self.position)
            .filter(|offset| *offset < BAG_SIZE)
    }

    /// Repairs the store, then moves the window back into range.
    ///
    /// Repair can shorten the used part of the store, which can leave the
    /// window pointing past everything.
    pub fn repair(&mut self) -> Vec<Item> {
        let overflow = self.store.repair();
        self.position = self.position.min(self.max_position());
        overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOTGUN: i32 = 6;
    const HERB: i32 = 30;

    fn item(id: i32) -> Item {
        Item { id, count: 1 }
    }

    fn empty_bag() -> Bag {
        Bag {
            unknown00: 0,
            items: [Item::EMPTY; BAG_SIZE],
            personal_item: Item::EMPTY,
            equipped_index: -1,
        }
    }

    fn filled_window(capacity: usize) -> Window {
        let mut window = Window::new(capacity);
        for i in 0..capacity {
            window.store_mut().set(i, item(HERB + i as i32));
        }
        window
    }

    #[test]
    fn a_store_the_size_of_the_bag_cannot_scroll() {
        let mut window = Window::new(BAG_SIZE);

        assert!(!window.is_scrollable());
        assert_eq!(window.max_position(), 0);
        assert!(!window.scroll_rows(1));
    }

    #[test]
    fn scrolling_moves_by_rows_of_two() {
        let mut window = filled_window(12);

        assert!(window.scroll_rows(1));
        assert_eq!(window.position(), 2);

        assert!(window.scroll_rows(2));
        assert_eq!(window.position(), 6);

        assert!(window.scroll_rows(-1));
        assert_eq!(window.position(), 4);
    }

    #[test]
    fn scrolling_clamps_at_both_ends_and_reports_no_movement() {
        let mut window = filled_window(10);

        assert!(!window.scroll_rows(-1));
        assert_eq!(window.position(), 0);

        assert!(window.scroll_rows(99));
        assert_eq!(window.position(), window.max_position());
        assert_eq!(window.position(), 4);

        assert!(!window.scroll_rows(1));
    }

    #[test]
    fn the_position_stays_even_however_it_is_reached() {
        let mut window = filled_window(16);

        for rows in [3, -1, 7, -2, 100, -100] {
            window.scroll_rows(rows);
            assert!(
                window.position().is_multiple_of(2),
                "position {} is odd after scrolling {rows}",
                window.position()
            );
        }
    }

    #[test]
    fn revealing_an_index_already_in_view_does_not_move() {
        let mut window = filled_window(12);
        window.scroll_rows(2);
        let before = window.position();

        window.reveal(before + 3);
        assert_eq!(window.position(), before);
    }

    #[test]
    fn revealing_scrolls_the_shortest_way_and_stays_even() {
        let mut window = filled_window(16);

        window.reveal(11);
        assert!(window.position().is_multiple_of(2));
        assert!(window.visible_slot(11).is_some());

        window.reveal(1);
        assert_eq!(window.position(), 0);
        assert!(window.visible_slot(1).is_some());
    }

    #[test]
    fn the_bag_shows_the_slots_under_the_window() {
        let mut window = filled_window(12);
        let mut bag = empty_bag();

        window.scroll_rows(2);
        window.write_into(&mut bag);

        for slot in 0..BAG_SIZE {
            let expected = window.store().get(window.position() + slot).unwrap();
            assert_eq!(bag.items[slot], expected);
        }
    }

    #[test]
    fn writing_the_bag_back_only_touches_the_visible_slots() {
        let mut window = filled_window(12);
        window.scroll_rows(1);

        let outside_before = window.store().get(0).unwrap();

        let mut bag = empty_bag();
        window.write_into(&mut bag);
        bag.items[0] = item(SHOTGUN);
        window.read_from(&bag);

        assert_eq!(window.store().get(2).unwrap(), item(SHOTGUN));
        assert_eq!(window.store().get(0).unwrap(), outside_before);
    }

    #[test]
    fn the_personal_item_and_equipped_index_are_left_alone() {
        let window = filled_window(12);
        let mut bag = empty_bag();
        bag.personal_item = item(SHOTGUN);
        bag.equipped_index = 3;

        window.write_into(&mut bag);

        assert_eq!(bag.personal_item, item(SHOTGUN));
        assert_eq!(bag.equipped_index, 3);
    }

    #[test]
    fn repair_pulls_the_window_back_into_range() {
        let mut window = Window::new(12);
        window.store_mut().set(10, item(HERB));
        window.scroll_rows(3);
        assert_eq!(window.position(), 6);

        window.repair();

        assert!(window.position() <= window.max_position());
        assert!(window.store().is_valid());
    }

    #[test]
    fn slot_and_index_translate_both_ways() {
        let mut window = filled_window(12);
        window.scroll_rows(2);

        assert_eq!(window.store_index(0), Some(4));
        assert_eq!(window.store_index(5), Some(9));
        assert_eq!(window.store_index(BAG_SIZE), None);

        assert_eq!(window.visible_slot(4), Some(0));
        assert_eq!(window.visible_slot(9), Some(5));
        assert_eq!(window.visible_slot(3), None);
        assert_eq!(window.visible_slot(10), None);
    }
}
