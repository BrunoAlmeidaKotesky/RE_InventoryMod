//! The game's inventory data structures.
//!
//! Layout is taken from third-party reverse engineering and is NOT yet
//! confirmed against this build. Nothing here may be written to game memory
//! until the probe has validated it in a live process.

/// Slots the game itself allocates per character.
pub const BAG_SIZE: usize = 6;

/// Filler placed in the second slot of a two-slot item.
pub const SLOT_TWO_FILLER: i32 = 180;

/// Items that occupy two slots. Their presence forces every design decision:
/// the inventory is addressed as rows of 2, and any slot count must be even.
pub const TWO_SLOT_ITEMS: [i32; 9] = [
    5,   // hunting gun
    6,   // shotgun
    7,   // grenade launcher, grenade rounds
    8,   // grenade launcher, flame rounds
    9,   // grenade launcher, acid rounds
    11,  // sub-machine gun
    12,  // invalid weapon, no name/icon/model
    23,  // rocket launcher
    104, // hookshot
];

pub fn is_two_slot(id: i32) -> bool {
    TWO_SLOT_ITEMS.contains(&id)
}

/// One inventory entry. 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Item {
    pub id: i32,
    pub count: i32,
}

// Used only from the mod's own store, which nothing calls into yet.
#[allow(dead_code)]
impl Item {
    /// An unoccupied slot. The game marks these with id zero.
    pub const EMPTY: Item = Item { id: 0, count: 0 };

    /// The entry that occupies the second slot of a two-slot item.
    pub const fn filler() -> Item {
        Item {
            id: SLOT_TWO_FILLER,
            count: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.id == Item::EMPTY.id
    }
}

/// A character's inventory as the game stores it. 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Bag {
    pub unknown00: i32,
    pub items: [Item; BAG_SIZE],
    pub personal_item: Item,
    pub equipped_index: i32,
}

/// Size assumed by every caller in the game that embeds a Bag inline.
pub const BAG_BYTES: usize = 64;

const _: () = assert!(std::mem::size_of::<Bag>() == BAG_BYTES);

impl Bag {
    /// Reads a Bag out of a raw 64-byte buffer.
    pub fn from_bytes(buf: &[u8; BAG_BYTES]) -> Bag {
        let at = |o: usize| i32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);

        let mut items = [Item::default(); BAG_SIZE];
        for (i, item) in items.iter_mut().enumerate() {
            let o = 0x04 + i * 8;
            *item = Item {
                id: at(o),
                count: at(o + 4),
            };
        }

        Bag {
            unknown00: at(0x00),
            items,
            personal_item: Item {
                id: at(0x34),
                count: at(0x38),
            },
            equipped_index: at(0x3C),
        }
    }

    /// Whether the contents are consistent with the game's own rules. Used to
    /// rank probe candidates: random memory almost never satisfies all of these.
    pub fn looks_plausible(&self) -> bool {
        if !(-1..BAG_SIZE as i32).contains(&self.equipped_index) {
            return false;
        }

        for (i, item) in self.items.iter().enumerate() {
            if item.id < 0 || item.id > 255 || item.count < 0 || item.count > 999 {
                return false;
            }

            // The filler only ever sits in the second half of a row.
            if item.id == SLOT_TWO_FILLER && i % 2 == 0 {
                return false;
            }

            // A two-slot item must start a row and be followed by the filler.
            if is_two_slot(item.id) {
                if i % 2 != 0 {
                    return false;
                }
                if self.items[i + 1].id != SLOT_TWO_FILLER {
                    return false;
                }
            }
        }

        true
    }
}
