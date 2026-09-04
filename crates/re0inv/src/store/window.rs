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

/// What the game stores in the equipped index when nothing is equipped.
const NOTHING_EQUIPPED: i32 = -1;

/// Rows in the window. Two slots each.
const ROWS: usize = BAG_SIZE / 2;

/// A visible row held on one store row while the other rows keep sliding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pin {
    visible_row: usize,
    store_row: usize,
}

pub struct Window {
    store: Slots,
    /// First store index the game can see. Always even.
    ///
    /// While a row is pinned, this counts through the store's rows with the
    /// pinned one left out, so the two free rows show consecutive rows of
    /// everything else; the bounds are unchanged either way.
    position: usize,
    /// Store index of the equipped item, if anything is equipped.
    ///
    /// The game keeps this as a slot number between 0 and 5, which only means
    /// anything relative to what the window is showing. Scrolling the contents
    /// underneath it without translating leaves the game holding a slot number
    /// that now points at a different item, or at nothing.
    equipped: Option<usize>,
    pin: Option<Pin>,
}

impl Window {
    pub fn new(capacity: usize) -> Window {
        Window {
            store: Slots::new(capacity),
            position: 0,
            equipped: None,
            pin: None,
        }
    }

    /// A window over the given items, at the given position.
    ///
    /// The capacity grows to fit the items rather than truncating them. The
    /// items come from a file recording what the player owned; a configuration
    /// that shrank since then is not a licence to delete things, and a larger
    /// window is the only answer that keeps everything.
    pub fn with_items(capacity: usize, items: &[Item], position: usize) -> Window {
        let mut window = Window::new(capacity.max(items.len()));

        for (index, item) in items.iter().enumerate() {
            window.store.set(index, *item);
        }

        window.set_position(position);
        window
    }

    /// Store index of the equipped item.
    pub fn equipped(&self) -> Option<usize> {
        self.equipped
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
        if self.pin.is_some() {
            if self.visible_slot(index).is_none() {
                if let Some(position) = self
                    .positions()
                    .find(|&position| self.visible_slot_at(position, index).is_some())
                {
                    self.position = position;
                }
            }
            return;
        }

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

    /// Moves the window to `position`, clamped and forced even.
    ///
    /// Even alignment is not cosmetic. A two-slot item occupies an even index
    /// and the filler after it an odd one, so a window starting on an odd index
    /// shows the tail of an item without its head.
    pub fn set_position(&mut self, position: usize) -> bool {
        let target = (position & !1).min(self.max_position());
        let changed = target != self.position;
        self.position = target;
        changed
    }

    /// Every position the window can rest at, in order.
    pub fn positions(&self) -> impl Iterator<Item = usize> {
        (0..=self.max_position()).step_by(2)
    }

    /// Copies the visible slots into the bag the game reads.
    ///
    /// The personal item is left alone: it is a slot of its own, outside the
    /// array, and has nothing to do with the window.
    ///
    /// The equipped index is rewritten, because it is a slot number and the
    /// slots have just changed underneath it. When the equipped item is out of
    /// view the game is told nothing is equipped, which is the only honest
    /// answer available in six slots.
    pub fn write_into(&self, bag: &mut Bag) {
        for slot in 0..BAG_SIZE {
            bag.items[slot] = self
                .store_index(slot)
                .and_then(|index| self.store.get(index))
                .unwrap_or(Item::EMPTY);
        }

        bag.equipped_index = self
            .equipped
            .and_then(|index| self.visible_slot(index))
            .map_or(NOTHING_EQUIPPED, |slot| slot as i32);
    }

    /// Copies whatever the game left in the bag back into the store.
    pub fn read_from(&mut self, bag: &Bag) {
        for slot in 0..BAG_SIZE {
            if let Some(index) = self.store_index(slot) {
                self.store.set(index, bag.items[slot]);
            }
        }

        let slot = bag.equipped_index;

        if (0..BAG_SIZE as i32).contains(&slot) {
            self.equipped = self.store_index(slot as usize);
            return;
        }

        // The game says nothing is equipped. Believe it only when what we think
        // is equipped is in view: otherwise this is our own `write_into` being
        // read back, and clearing here would unequip the item for good.
        let equipped_is_visible = self
            .equipped
            .is_some_and(|index| self.visible_slot(index).is_some());

        if equipped_is_visible {
            self.equipped = None;
        }
    }

    /// First visible slot that is empty, if any.
    ///
    /// This is what a caller can safely be told to write into: the game can
    /// only address the six slots it has.
    pub fn first_visible_empty(&self) -> Option<usize> {
        (0..BAG_SIZE).find(|&slot| self.slot_is_empty_at(self.position, slot))
    }

    fn slot_is_empty_at(&self, position: usize, slot: usize) -> bool {
        self.store_index_at(position, slot)
            .and_then(|index| self.store.get(index))
            .is_none_or(|item| item.is_empty())
    }

    /// How many of the slots in view are empty.
    pub fn visible_empty_count(&self) -> usize {
        self.empty_in_view_at(self.position)
    }

    /// The position showing the most empty slots at once, and how many.
    ///
    /// The current position wins a tie, so the window does not jump when it
    /// already has the room; otherwise the earliest such position.
    ///
    /// This is the most room the game can ever be promised. The store may hold
    /// more empty slots in total, but the game writes into the six it can see,
    /// and its own code trusts what `count_empty` said to fit in them.
    pub fn roomiest_position(&self) -> (usize, usize) {
        let mut best = (self.position, self.visible_empty_count());

        for position in self.positions() {
            let count = self.empty_in_view_at(position);
            if count > best.1 {
                best = (position, count);
            }
        }

        best
    }

    fn empty_in_view_at(&self, position: usize) -> usize {
        (0..BAG_SIZE)
            .filter(|&slot| self.slot_is_empty_at(position, slot))
            .count()
    }

    /// Packs the store towards the front, keeping the equipped item equipped.
    ///
    /// The game's own `organize` does this to the six slots it can see after
    /// every change; nothing does it to the rest of the store, so a hole left
    /// on one page stays. Two holes on different pages are two free slots no
    /// window shows together, which refuses a two-slot item with room to
    /// spare. Packing is exactly what the game would have done to a six-slot
    /// bag.
    ///
    /// The equipped item is found again by identity afterwards. If the repair
    /// cannot fit everything — which a valid store of unchanged capacity never
    /// triggers — nothing is changed, rather than lose an item.
    pub fn compact(&mut self) {
        // Nothing holds a slot number across a pickup, and a held row would
        // point at whatever the packing moved there.
        self.pin = None;

        let before = self.store.clone();
        let equipped = self.equipped.and_then(|index| self.store.get(index));

        if !self.store.repair().is_empty() {
            self.store = before;
            return;
        }

        self.equipped = equipped.and_then(|item| {
            self.store
                .as_slice()
                .iter()
                .position(|candidate| *candidate == item)
        });
        self.position = self.position.min(self.max_position());
    }

    /// Store index currently shown in visible slot `slot`.
    pub fn store_index(&self, slot: usize) -> Option<usize> {
        self.store_index_at(self.position, slot)
    }

    /// Visible slot showing store index `index`, if it is in view.
    pub fn visible_slot(&self, index: usize) -> Option<usize> {
        self.visible_slot_at(self.position, index)
    }

    /// Store index that visible slot `slot` would show at `position`.
    ///
    /// Without a pin the window is six consecutive slots. With one, the pinned
    /// visible row always shows the pinned store row, and the other two show
    /// consecutive rows of the store with the pinned row left out, counted
    /// from `position`.
    fn store_index_at(&self, position: usize, slot: usize) -> Option<usize> {
        if slot >= BAG_SIZE {
            return None;
        }

        let (row, column) = (slot / 2, slot % 2);

        let Some(pin) = self.pin else {
            return Some(position + slot);
        };

        if row == pin.visible_row {
            return Some(pin.store_row * 2 + column);
        }

        let order = if row < pin.visible_row { row } else { row - 1 };
        let sequence_row = position / 2 + order;
        let store_row = if sequence_row < pin.store_row {
            sequence_row
        } else {
            sequence_row + 1
        };

        Some(store_row * 2 + column)
    }

    /// Visible slot that store index `index` would take at `position`.
    fn visible_slot_at(&self, position: usize, index: usize) -> Option<usize> {
        let Some(pin) = self.pin else {
            return index
                .checked_sub(position)
                .filter(|offset| *offset < BAG_SIZE);
        };

        let (row, column) = (index / 2, index % 2);

        if row == pin.store_row {
            return Some(pin.visible_row * 2 + column);
        }

        let sequence_row = if row < pin.store_row { row } else { row - 1 };
        let order = sequence_row.checked_sub(position / 2)?;
        if order >= ROWS - 1 {
            return None;
        }

        let visible_row = if order < pin.visible_row { order } else { order + 1 };
        Some(visible_row * 2 + column)
    }

    /// Holds visible row `visible_row` on the store row it shows now, while
    /// the other rows keep scrolling over the rest of the store.
    ///
    /// For the inventory's two-item actions. Confirming the first item saves
    /// its visible slot in the menu, and the action later reads both slots
    /// from the six the game can see. Scrolling to reach a second item
    /// anywhere else in the store would slide a different item under the
    /// saved slot — unless that slot's row stays where it is. So it does.
    ///
    /// The first unpinned row keeps showing what it shows now, so nothing
    /// visibly jumps at the moment of pinning.
    pub fn pin_row(&mut self, visible_row: usize) {
        if visible_row >= ROWS || self.pin.is_some() {
            return;
        }

        let store_row = self.position / 2 + visible_row;
        let first_other = usize::from(visible_row == 0);
        let shown = self.position / 2 + first_other;
        let sequence_row = if shown < store_row { shown } else { shown - 1 };

        self.pin = Some(Pin {
            visible_row,
            store_row,
        });
        self.position = (sequence_row * 2).min(self.max_position());
    }

    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    /// Releases the held row.
    ///
    /// `keep` names a visible slot whose item should stay where it is on
    /// screen, if the plain window can show it there; otherwise the item is
    /// merely kept in view.
    pub fn unpin(&mut self, keep: Option<usize>) {
        if self.pin.is_none() {
            return;
        }

        let kept = keep.and_then(|slot| self.store_index(slot).map(|index| (slot, index)));
        self.pin = None;

        match kept {
            Some((slot, index)) => {
                let same_place = index
                    .checked_sub(slot)
                    .filter(|position| position.is_multiple_of(2) && *position <= self.max_position());

                match same_place {
                    Some(position) => self.position = position,
                    None => self.reveal(index),
                }
            }
            None => self.position = self.position.min(self.max_position()),
        }
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
    fn the_personal_item_is_left_alone() {
        let window = filled_window(12);
        let mut bag = empty_bag();
        bag.personal_item = item(SHOTGUN);

        window.write_into(&mut bag);

        assert_eq!(bag.personal_item, item(SHOTGUN));
    }

    #[test]
    fn the_equipped_item_follows_the_window() {
        let mut window = filled_window(12);
        let mut bag = empty_bag();

        // The game equips what is in visible slot 2, which is store index 2.
        bag.equipped_index = 2;
        window.read_from(&bag);
        assert_eq!(window.equipped(), Some(2));

        // Scrolling one row leaves it in view, one slot earlier.
        window.scroll_rows(1);
        window.write_into(&mut bag);
        assert_eq!(bag.equipped_index, 0);

        // Scrolling past it reports nothing equipped, without forgetting it.
        window.scroll_rows(1);
        window.write_into(&mut bag);
        assert_eq!(bag.equipped_index, -1);
        assert_eq!(window.equipped(), Some(2));

        // Coming back brings it into view again.
        window.scroll_rows(-2);
        window.write_into(&mut bag);
        assert_eq!(bag.equipped_index, 2);
    }

    #[test]
    fn an_out_of_view_equipped_item_survives_a_read_back() {
        let mut window = filled_window(12);
        let mut bag = empty_bag();

        bag.equipped_index = 0;
        window.read_from(&bag);
        window.scroll_rows(2);
        window.write_into(&mut bag);

        // This is our own -1 coming back, not the game unequipping.
        window.read_from(&bag);
        assert_eq!(window.equipped(), Some(0));
    }

    #[test]
    fn the_game_unequipping_in_view_is_believed() {
        let mut window = filled_window(12);
        let mut bag = empty_bag();

        bag.equipped_index = 1;
        window.read_from(&bag);
        assert_eq!(window.equipped(), Some(1));

        bag.equipped_index = -1;
        window.read_from(&bag);
        assert_eq!(window.equipped(), None);
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
    fn a_free_slot_in_view_is_reported_by_its_visible_position() {
        let mut window = Window::new(12);
        window.store_mut().set(0, item(HERB));
        window.store_mut().set(1, item(HERB));

        assert_eq!(window.first_visible_empty(), Some(2));
    }

    #[test]
    fn a_full_window_reports_nothing_visible_even_with_room_behind_it() {
        let mut window = Window::new(12);
        for i in 0..BAG_SIZE {
            window.store_mut().set(i, item(HERB));
        }

        assert_eq!(window.first_visible_empty(), None);
        assert_eq!(window.store().first_empty(), Some(BAG_SIZE));
    }

    /// The index these lookups produce is used by the game as a write target,
    /// so it has to name one of the six slots the bag actually has. Anything
    /// larger would write past the array and over the personal item.
    #[test]
    fn revealing_a_slot_behind_the_window_yields_a_writable_index() {
        let mut window = Window::new(12);
        for i in 0..BAG_SIZE {
            window.store_mut().set(i, item(HERB));
        }

        let index = window.store().first_empty().unwrap();
        window.reveal(index);

        let slot = window.visible_slot(index).expect("revealed but not visible");
        assert!(slot < BAG_SIZE, "slot {slot} is outside the bag");
        assert_eq!(window.store_index(slot), Some(index));
    }

    #[test]
    fn every_store_index_becomes_writable_once_revealed() {
        let capacity = 16;
        let mut window = Window::new(capacity);

        for index in 0..capacity {
            window.reveal(index);
            let slot = window
                .visible_slot(index)
                .unwrap_or_else(|| panic!("index {index} not visible after reveal"));
            assert!(slot < BAG_SIZE, "index {index} mapped to slot {slot}");
        }
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

    /// Nine items in twelve slots, window on slots 4..10: one empty in view,
    /// three in the store. The state that once took the game down.
    fn nine_of_twelve() -> Window {
        let mut window = Window::new(12);
        for i in 0..9 {
            window.store_mut().set(i, item(HERB));
        }
        window.set_position(4);
        window
    }

    #[test]
    fn the_roomiest_position_is_where_the_empties_are() {
        let window = nine_of_twelve();

        assert_eq!(window.visible_empty_count(), 1);
        assert_eq!(window.roomiest_position(), (6, 3));
    }

    #[test]
    fn the_current_position_keeps_a_tie() {
        let mut window = Window::new(12);
        window.set_position(4);
        assert_eq!(window.roomiest_position(), (4, BAG_SIZE));

        let full = filled_window(12);
        assert_eq!(full.roomiest_position(), (0, 0));
    }

    #[test]
    fn compacting_closes_holes_and_keeps_the_equipped_item() {
        let mut window = Window::new(12);
        for i in [0, 1, 2, 3, 4, 6, 7, 8, 9, 10] {
            window.store_mut().set(i, item(HERB + i as i32));
        }
        window.set_position(6);

        let mut bag = empty_bag();
        window.write_into(&mut bag);
        bag.equipped_index = 2; // store index 8
        window.read_from(&bag);
        assert_eq!(window.equipped(), Some(8));

        window.compact();

        assert_eq!(window.store().count_empty(), 2);
        assert!(window.store().get(10).unwrap().is_empty());
        assert!(window.store().get(11).unwrap().is_empty());
        assert_eq!(window.equipped(), Some(7));
        assert_eq!(window.store().get(7).unwrap(), item(HERB + 8));
    }

    /// Twelve slots holding their own index as id, so a slot's origin is
    /// readable from its id.
    fn numbered(capacity: usize) -> Window {
        let mut window = Window::new(capacity);
        for i in 0..capacity {
            window.store_mut().set(i, item(i as i32));
        }
        window
    }

    fn shown(window: &Window) -> Vec<i32> {
        let mut bag = empty_bag();
        window.write_into(&mut bag);
        bag.items.iter().map(|item| item.id).collect()
    }

    #[test]
    fn pinning_keeps_the_row_while_the_rest_scrolls() {
        let mut window = numbered(12);
        window.pin_row(1);

        // Nothing jumps at the moment of pinning.
        assert_eq!(shown(&window), vec![0, 1, 2, 3, 4, 5]);

        assert!(window.scroll_rows(1));
        assert_eq!(shown(&window), vec![4, 5, 2, 3, 6, 7]);

        assert!(window.scroll_rows(99));
        assert_eq!(shown(&window), vec![8, 9, 2, 3, 10, 11]);
        assert_eq!(window.position(), window.max_position());

        // Both directions of the mapping agree everywhere.
        for slot in 0..BAG_SIZE {
            let index = window.store_index(slot).unwrap();
            assert_eq!(window.visible_slot(index), Some(slot));
        }
        assert_eq!(window.visible_slot(2), Some(2));
        assert_eq!(window.visible_slot(0), None);
    }

    #[test]
    fn a_pinned_window_reads_back_into_the_right_slots() {
        let mut window = numbered(12);
        window.pin_row(0);
        window.scroll_rows(2);
        assert_eq!(shown(&window), vec![0, 1, 6, 7, 8, 9]);

        let mut bag = empty_bag();
        window.write_into(&mut bag);
        bag.items[1] = item(SHOTGUN);
        bag.items[5] = item(HERB);
        window.read_from(&bag);

        assert_eq!(window.store().get(1).unwrap(), item(SHOTGUN));
        assert_eq!(window.store().get(9).unwrap(), item(HERB));
        assert_eq!(window.store().get(3).unwrap(), item(3));
    }

    #[test]
    fn unpinning_keeps_the_kept_slot_where_it_is() {
        let mut window = numbered(12);
        window.pin_row(1);
        window.scroll_rows(99);
        assert_eq!(shown(&window), vec![8, 9, 2, 3, 10, 11]);

        // Slot 4 shows store index 10; the plain window at position 6 too.
        window.unpin(Some(4));
        assert!(!window.is_pinned());
        assert_eq!(window.position(), 6);
        assert_eq!(shown(&window), vec![6, 7, 8, 9, 10, 11]);

        let mut again = numbered(12);
        again.pin_row(2);
        again.scroll_rows(99);
        // Slot 4 is the pinned row: store index 4 cannot sit at slot 4 with an
        // even position past zero, so it is merely kept in view.
        again.unpin(Some(4));
        assert!(again.visible_slot(4).is_some());
    }

    #[test]
    fn pinning_covers_every_row_of_a_six_slot_store() {
        let mut window = numbered(6);
        window.pin_row(2);
        assert_eq!(shown(&window), vec![0, 1, 2, 3, 4, 5]);
        assert!(!window.scroll_rows(1));
    }

    #[test]
    fn pinned_empties_and_reveal_use_the_same_mapping() {
        let mut window = numbered(12);
        // Id zero is the empty item; give slot 0 a real one.
        window.store_mut().set(0, item(HERB));
        window.store_mut().set(11, Item::EMPTY);
        window.pin_row(0);

        assert_eq!(window.first_visible_empty(), None);
        window.reveal(11);
        assert_eq!(window.visible_slot(11), Some(5));
        assert_eq!(window.first_visible_empty(), Some(5));
    }
}
