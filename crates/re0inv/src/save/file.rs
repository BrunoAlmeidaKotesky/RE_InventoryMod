//! The side file that holds what the game's save has no room for.
//!
//! # Why beside the save and not inside it
//!
//! The game's save could carry this: append a block after the vanilla bytes and
//! read it back. That is what the mod this borrows its research from does, and
//! it travels with Steam Cloud, which a file on disk does not.
//!
//! It is also the version where a mistake costs the player their progress. A
//! side file cannot do that. The worst it can do is lose what this mod added,
//! which is a bad afternoon rather than a lost campaign, and that trade is not
//! close enough to think about twice.
//!
//! # Layout
//!
//! ```text
//! offset  size  field
//!    +0      6  "RE0INV"
//!    +6      2  format version, u16
//!    +8      4  number of slot records, u32
//!   +12      4  CRC32 of everything after this field
//!   +16      N  the records
//! ```
//!
//! Each record:
//!
//! ```text
//!    +0      1  save slot, 0..19
//!    +1      1  number of stores in it
//!    +2      2  items in the box, u16
//!    +4      N  the box's items
//!         then  one store per character
//! ```
//!
//! Each store:
//!
//! ```text
//!    +0      4  the bag's offset inside its owner, u32 (0x20 or 0x60)
//!    +4      2  window position when this was written, u16
//!    +6      2  number of items, u16
//!    +8      N  the items
//! ```
//!
//! An item is two little-endian `i32`: the id, then the count.
//!
//! # Why the window position is recorded
//!
//! The game's own save holds the six slots it can see, which are the six the
//! window happened to be over. Restoring the rest without knowing where that
//! window was would put the saved six in the wrong place. On load the recorded
//! six are compared against what the game actually restored, and a store is only
//! trusted when they agree.

use crate::game::inventory::Item;

const MAGIC: &[u8; 6] = b"RE0INV";
const VERSION: u16 = 1;
const HEADER: usize = 16;

/// Everything one save slot holds beyond what the game records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotData {
    pub slot: u8,
    pub box_items: Vec<Item>,
    pub stores: Vec<StoreData>,
}

/// One character's inventory, and where its window was.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreData {
    /// Which of the two inline bags this is, `0x20` or `0x60`.
    pub offset: u32,
    pub position: u16,
    pub items: Vec<Item>,
}

impl StoreData {
    /// The six the game would have saved, given where the window was.
    ///
    /// Used to tell a side file that belongs to this save from one left over
    /// from a different session.
    #[cfg(test)]
    pub fn visible(&self, count: usize) -> Vec<Item> {
        (0..count)
            .map(|slot| {
                self.items
                    .get(self.position as usize + slot)
                    .copied()
                    .unwrap_or(Item::EMPTY)
            })
            .collect()
    }
}

/// Every save slot this file knows about.
#[derive(Clone, Debug, Default)]
pub struct SaveFile {
    pub slots: Vec<SlotData>,
}

impl SaveFile {
    pub fn get(&self, slot: u8) -> Option<&SlotData> {
        self.slots.iter().find(|record| record.slot == slot)
    }

    /// Takes one slot's record out, so its contents can be moved rather than
    /// cloned.
    pub fn take(&mut self, slot: u8) -> Option<SlotData> {
        let index = self.slots.iter().position(|record| record.slot == slot)?;
        Some(self.slots.swap_remove(index))
    }

    /// Replaces one slot's record, adding it if it is new.
    pub fn put(&mut self, data: SlotData) {
        match self.slots.iter_mut().find(|record| record.slot == data.slot) {
            Some(existing) => *existing = data,
            None => self.slots.push(data),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();

        for record in &self.slots {
            // The count is one byte, so what is written past it has to agree
            // with it: a count that says 255 while the body holds more would
            // decode the extras as the next record, which is garbage that
            // passes the checksum.
            let stores = &record.stores[..record.stores.len().min(u8::MAX as usize)];

            body.push(record.slot);
            body.push(stores.len() as u8);
            put_u16(&mut body, record.box_items.len() as u16);
            put_items(&mut body, &record.box_items);

            for store in stores {
                put_u32(&mut body, store.offset);
                put_u16(&mut body, store.position);
                put_u16(&mut body, store.items.len() as u16);
                put_items(&mut body, &store.items);
            }
        }

        let mut out = Vec::with_capacity(HEADER + body.len());
        out.extend_from_slice(MAGIC);
        put_u16(&mut out, VERSION);
        put_u32(&mut out, self.slots.len() as u32);
        put_u32(&mut out, crc32(&body));
        out.extend_from_slice(&body);
        out
    }

    /// Reads a side file, refusing anything it does not fully understand.
    ///
    /// Every failure returns an error rather than a partial result. Restoring
    /// half a record would put items in the wrong slots, which is worse than
    /// restoring none: the player would not know anything had gone wrong.
    pub fn decode(bytes: &[u8]) -> Result<SaveFile, String> {
        if bytes.len() < HEADER {
            return Err("too short to be a side file".into());
        }

        if &bytes[..6] != MAGIC {
            return Err("not a side file for this mod".into());
        }

        let version = u16(bytes, 6);
        if version != VERSION {
            return Err(format!("format version {version}, this build reads {VERSION}"));
        }

        let count = u32(bytes, 8) as usize;
        let expected_crc = u32(bytes, 12);
        let body = &bytes[HEADER..];

        // The checksum covers the body, not this header field, so the count is
        // still unverified input here. Handing it to `with_capacity` unchecked
        // lets sixteen corrupt bytes request gigabytes, and a failed allocation
        // is an abort no catch_unwind can turn into a log line. The smallest a
        // record can be is four bytes, which bounds any honest count.
        if count > body.len() / 4 {
            return Err(format!("{count} records cannot fit in {} bytes", body.len()));
        }

        let actual = crc32(body);
        if actual != expected_crc {
            return Err(format!(
                "checksum is 0x{actual:08X}, the file says 0x{expected_crc:08X}"
            ));
        }

        let mut at = 0;
        let mut slots = Vec::with_capacity(count);

        for _ in 0..count {
            let slot = take_u8(body, &mut at)?;
            let stores = take_u8(body, &mut at)? as usize;
            let box_len = take_u16(body, &mut at)? as usize;
            let box_items = take_items(body, &mut at, box_len)?;

            let mut records = Vec::with_capacity(stores);
            for _ in 0..stores {
                let offset = take_u32(body, &mut at)?;
                let position = take_u16(body, &mut at)?;
                let len = take_u16(body, &mut at)? as usize;
                let items = take_items(body, &mut at, len)?;

                records.push(StoreData {
                    offset,
                    position,
                    items,
                });
            }

            slots.push(SlotData {
                slot,
                box_items,
                stores: records,
            });
        }

        Ok(SaveFile { slots })
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_items(out: &mut Vec<u8>, items: &[Item]) {
    for item in items {
        out.extend_from_slice(&item.id.to_le_bytes());
        out.extend_from_slice(&item.count.to_le_bytes());
    }
}

fn u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn take_u8(bytes: &[u8], at: &mut usize) -> Result<u8, String> {
    let value = *bytes.get(*at).ok_or("ran off the end of the file")?;
    *at += 1;
    Ok(value)
}

fn take_u16(bytes: &[u8], at: &mut usize) -> Result<u16, String> {
    if *at + 2 > bytes.len() {
        return Err("ran off the end of the file".into());
    }
    let value = u16(bytes, *at);
    *at += 2;
    Ok(value)
}

fn take_u32(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    if *at + 4 > bytes.len() {
        return Err("ran off the end of the file".into());
    }
    let value = u32(bytes, *at);
    *at += 4;
    Ok(value)
}

fn take_items(bytes: &[u8], at: &mut usize, count: usize) -> Result<Vec<Item>, String> {
    if *at + count * 8 > bytes.len() {
        return Err("ran off the end of the file".into());
    }

    let items = (0..count)
        .map(|index| {
            let base = *at + index * 8;
            Item {
                id: u32(bytes, base) as i32,
                count: u32(bytes, base + 4) as i32,
            }
        })
        .collect();

    *at += count * 8;
    Ok(items)
}

/// CRC32, the same polynomial zip and png use.
///
/// Written out rather than pulled in: it is fifteen lines, and a dependency
/// that has to build for a 32-bit Windows target is fifteen lines of trouble.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i32, count: i32) -> Item {
        Item { id, count }
    }

    fn sample() -> SaveFile {
        SaveFile {
            slots: vec![
                SlotData {
                    slot: 3,
                    box_items: vec![item(5, 1), item(180, 0), item(42, 30)],
                    stores: vec![
                        StoreData {
                            offset: 0x20,
                            position: 2,
                            items: vec![item(1, 1), item(2, 2), item(3, 3)],
                        },
                        StoreData {
                            offset: 0x60,
                            position: 0,
                            items: vec![item(9, 9)],
                        },
                    ],
                },
                SlotData {
                    slot: 7,
                    box_items: vec![],
                    stores: vec![],
                },
            ],
        }
    }

    #[test]
    fn round_trips() {
        let encoded = sample().encode();
        let decoded = SaveFile::decode(&encoded).expect("should decode");

        assert_eq!(decoded.slots.len(), 2);
        assert_eq!(decoded.get(3), sample().get(3));
        assert_eq!(decoded.get(7), sample().get(7));
    }

    #[test]
    fn rejects_a_corrupted_body() {
        let mut encoded = sample().encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;

        assert!(SaveFile::decode(&encoded).is_err());
    }

    #[test]
    fn rejects_a_foreign_file() {
        assert!(SaveFile::decode(b"IBOXsomething else entirely").is_err());
    }

    #[test]
    fn rejects_a_truncated_file() {
        let encoded = sample().encode();
        assert!(SaveFile::decode(&encoded[..encoded.len() - 8]).is_err());
    }

    #[test]
    fn put_replaces_rather_than_duplicating() {
        let mut file = sample();
        file.put(SlotData {
            slot: 3,
            box_items: vec![item(1, 1)],
            stores: vec![],
        });

        assert_eq!(file.slots.len(), 2);
        assert_eq!(file.get(3).unwrap().box_items, vec![item(1, 1)]);
    }

    #[test]
    fn take_removes_and_returns_the_record() {
        let mut file = sample();
        let taken = file.take(3);

        assert_eq!(taken, sample().get(3).cloned());
        assert!(file.get(3).is_none());
        assert!(file.get(7).is_some());
    }

    #[test]
    fn rejects_a_count_larger_than_the_body() {
        let mut encoded = sample().encode();
        // The record count lives at offset 8, outside the checksum.
        encoded[8..12].copy_from_slice(&0x0800_0000u32.to_le_bytes());

        assert!(SaveFile::decode(&encoded).is_err());
    }

    #[test]
    fn visible_reads_through_the_window() {
        let store = StoreData {
            offset: 0x20,
            position: 2,
            items: vec![item(1, 1), item(2, 2), item(3, 3), item(4, 4)],
        };

        let visible = store.visible(3);
        assert_eq!(visible, vec![item(3, 3), item(4, 4), Item::EMPTY]);
    }
}
