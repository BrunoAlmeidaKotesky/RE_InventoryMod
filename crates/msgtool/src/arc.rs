//! The MT Framework archive format, as far as this project needs it.
//!
//! ```text
//! offset  size  what
//! 0x00    4     "ARC\0"
//! 0x04    2     version, 7 in this game
//! 0x06    2     number of entries
//! 0x08    80    first entry, then one every 80 bytes
//! ```
//!
//! Each entry:
//!
//! ```text
//! 0x00    64    path inside the archive, without an extension, null-padded
//! 0x40    4     hash of the extension, which names the file type
//! 0x44    4     compressed size
//! 0x48    4     decompressed size in the low bits, flags in the top ones
//! 0x4C    4     offset of the compressed data
//! ```
//!
//! Payloads are zlib streams. They are stored in offset order after the table,
//! the first one aligned well past the end of it.

use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"ARC\0";
const HEADER: usize = 8;
const ENTRY: usize = 0x50;
const NAME: usize = 0x40;

/// Where the first payload starts. Taken from the game's own archives rather
/// than computed, so a rebuilt archive is laid out the way the originals are.
const DATA_START: usize = 0x8000;

/// The decompressed size shares its word with flags. Every entry in the game's
/// message archives has `0x40000000` set, and the sizes are far below the
/// remaining range.
const SIZE_MASK: u32 = 0x3FFF_FFFF;

pub struct Entry {
    pub name: String,
    pub extension: u32,
    /// Flags kept exactly as they were, since what they mean is not known.
    pub flags: u32,
    pub data: Vec<u8>,
}

pub struct Archive {
    pub version: u16,
    pub entries: Vec<Entry>,
}

impl Archive {
    /// Reads an archive, decompressing every entry.
    pub fn read(bytes: &[u8]) -> Result<Archive, String> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return Err("not an ARC archive".into());
        }

        let version = u16(bytes, 4);
        let count = u16(bytes, 6) as usize;

        let table_end = HEADER + count * ENTRY;
        if bytes.len() < table_end {
            return Err(format!("truncated table: {count} entries do not fit"));
        }

        let mut entries = Vec::with_capacity(count);

        for index in 0..count {
            let at = HEADER + index * ENTRY;

            let name = String::from_utf8_lossy(&bytes[at..at + NAME])
                .trim_end_matches('\0')
                .to_string();

            let extension = u32(bytes, at + 0x40);
            let compressed = u32(bytes, at + 0x44) as usize;
            let size_word = u32(bytes, at + 0x48);
            let offset = u32(bytes, at + 0x4C) as usize;

            let size = (size_word & SIZE_MASK) as usize;
            let flags = size_word & !SIZE_MASK;

            let end = offset + compressed;
            if end > bytes.len() {
                return Err(format!("entry '{name}' runs past the end of the file"));
            }

            let data = inflate(&bytes[offset..end], size)
                .map_err(|e| format!("entry '{name}': {e}"))?;

            entries.push(Entry {
                name,
                extension,
                flags,
                data,
            });
        }

        Ok(Archive { version, entries })
    }

    /// Writes the archive back out, compressing every entry again.
    ///
    /// The result is not byte-identical to the original even when nothing
    /// changed: the compressor used here is not the one Capcom used, so the
    /// streams differ. The game only reads the sizes recorded in the table, so
    /// that is fine, but it does mean a rebuilt archive cannot be compared with
    /// the original by hash.
    pub fn write(&self) -> Vec<u8> {
        let mut table = vec![0u8; HEADER + self.entries.len() * ENTRY];
        table[..4].copy_from_slice(MAGIC);
        table[4..6].copy_from_slice(&self.version.to_le_bytes());
        table[6..8].copy_from_slice(&(self.entries.len() as u16).to_le_bytes());

        let mut payload = Vec::new();
        let mut offset = DATA_START;

        for (index, entry) in self.entries.iter().enumerate() {
            let compressed = deflate(&entry.data);
            let at = HEADER + index * ENTRY;

            let mut name = [0u8; NAME];
            let bytes = entry.name.as_bytes();
            let len = bytes.len().min(NAME - 1);
            name[..len].copy_from_slice(&bytes[..len]);

            table[at..at + NAME].copy_from_slice(&name);
            put(&mut table, at + 0x40, entry.extension);
            put(&mut table, at + 0x44, compressed.len() as u32);
            put(&mut table, at + 0x48, entry.data.len() as u32 | entry.flags);
            put(&mut table, at + 0x4C, offset as u32);

            offset += compressed.len();
            payload.extend_from_slice(&compressed);
        }

        let mut out = table;
        out.resize(DATA_START, 0);
        out.extend_from_slice(&payload);
        out
    }

    pub fn find(&mut self, name: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.name == name)
    }
}

fn inflate(bytes: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected);
    flate2::read::ZlibDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;

    if out.len() != expected {
        return Err(format!(
            "decompressed to {} bytes, the table says {expected}",
            out.len()
        ));
    }

    Ok(out)
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes).expect("writing to a Vec cannot fail");
    encoder.finish().expect("writing to a Vec cannot fail")
}

fn u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
