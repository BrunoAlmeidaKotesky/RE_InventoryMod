//! The message file that lives inside the archive.
//!
//! ```text
//! offset  what
//! 0x00    "GMD\0"
//! 0x14    number of labels
//! 0x18    number of strings
//! 0x1C    size of the label block
//! 0x20    size of the string block
//! ```
//!
//! The strings are the last thing in the file, one after another, each ending
//! in a null byte. There is no offset table pointing into them, which is why a
//! string can grow or shrink as long as the size at `0x20` is corrected: every
//! reader walks the block from the front.
//!
//! That was checked rather than assumed. Rebuilding an untouched file here
//! reproduces the original byte for byte, and `verify` exists to keep proving it.

/// Number of strings in the block.
const OFFSET_COUNT: usize = 0x18;
/// Size of the string block, in bytes.
const OFFSET_BLOCK_SIZE: usize = 0x20;

/// How much of the header to report while parts of it are still unnamed.
const HEADER_WORDS: usize = 12;

pub struct Gmd {
    /// Everything before the string block, kept exactly as it was read.
    head: Vec<u8>,
    strings: Vec<Vec<u8>>,
}

impl Gmd {
    pub fn read(bytes: &[u8]) -> Result<Gmd, String> {
        if bytes.len() < OFFSET_BLOCK_SIZE + 4 || &bytes[..4] != b"GMD\0" {
            return Err("not a GMD message file".into());
        }

        let block = u32(bytes, OFFSET_BLOCK_SIZE) as usize;
        if block > bytes.len() {
            return Err(format!(
                "the string block is {block} bytes but the file is only {}",
                bytes.len()
            ));
        }

        let split = bytes.len() - block;
        let head = bytes[..split].to_vec();

        // The block ends with the last string's terminator, so splitting on
        // nulls leaves an empty piece at the end that is not a string.
        let mut strings: Vec<Vec<u8>> = bytes[split..]
            .split(|&b| b == 0)
            .map(<[u8]>::to_vec)
            .collect();

        if strings.last().is_some_and(Vec::is_empty) {
            strings.pop();
        }

        let expected = u32(bytes, OFFSET_COUNT) as usize;
        if strings.len() != expected {
            return Err(format!(
                "found {} strings, the header says {expected}",
                strings.len()
            ));
        }

        Ok(Gmd { head, strings })
    }

    pub fn strings(&self) -> Vec<String> {
        self.strings
            .iter()
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect()
    }

    pub fn get(&self, index: usize) -> Option<String> {
        self.strings
            .get(index)
            .map(|s| String::from_utf8_lossy(s).into_owned())
    }

    /// Replaces one string. The block may change size; `write` corrects the
    /// header for it.
    pub fn set(&mut self, index: usize, text: &str) -> Result<(), String> {
        let slot = self
            .strings
            .get_mut(index)
            .ok_or_else(|| format!("no string at index {index}"))?;

        *slot = text.as_bytes().to_vec();
        Ok(())
    }

    /// Writes the file back out, with the string block size corrected.
    pub fn write(&self) -> Vec<u8> {
        let mut block = Vec::new();
        for text in &self.strings {
            block.extend_from_slice(text);
            block.push(0);
        }

        let mut out = self.head.clone();
        put(&mut out, OFFSET_BLOCK_SIZE, block.len() as u32);
        out.extend_from_slice(&block);
        out
    }

    /// The header, for the fields that are still only known by their offset.
    pub fn describe(&self, total: usize) -> String {
        let mut out = format!("{total} bytes, {} strings\n  header:", self.strings.len());

        for index in 0..HEADER_WORDS {
            let at = index * 4;
            if at + 4 > self.head.len() {
                break;
            }
            out.push_str(&format!(" [0x{at:02X}]={}", u32(&self.head, at)));
        }

        out
    }
}

/// Proves a file survives a read and a write unchanged.
///
/// Worth its own command. Everything this tool does to a message file rests on
/// the claim that the strings can be walked and rebuilt with nothing else in
/// the file depending on where they sit, and the only honest way to hold that
/// claim is to keep checking it against the player's own files rather than
/// against the ones it was worked out on.
pub fn verify(bytes: &[u8]) -> Result<(), String> {
    let file = Gmd::read(bytes)?;
    let out = file.write();

    if out == bytes {
        return Ok(());
    }

    let at = out
        .iter()
        .zip(bytes)
        .position(|(a, b)| a != b)
        .unwrap_or(out.len().min(bytes.len()));

    Err(format!(
        "rebuilding changed the file: {} bytes in, {} out, first difference at 0x{at:X}",
        bytes.len(),
        out.len()
    ))
}

fn u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
