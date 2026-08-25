//! Reads and rebuilds the game's message archives.
//!
//! The item box needs a third choice on the typewriter prompt, and what the
//! prompt offers is decided by the game's message files rather than by its
//! code. So there is no way to add one without touching that text.
//!
//! What this crate never does is modify the player's own files. It reads
//! `msg_<lang>.arc`, edits a copy in memory, and writes the result out under a
//! different name that the mod redirects to at runtime. The originals stay
//! exactly as Steam installed them, so verifying the game files reports
//! nothing and uninstalling is a delete.
//!
//! It is both a library and a command-line tool: the mod itself builds the
//! archives on first run, so installing is nothing but copying files, and the
//! binary exists for inspecting archives while developing.

use std::path::{Path, PathBuf};

pub mod arc;
pub mod gmd;
pub mod typewriter;

/// Suffix for the archives this crate writes.
///
/// Deliberately not `_box`, which is what re0box writes. Both mods redirect the
/// game to their own copy of the same file, and sharing a name would mean
/// whichever loaded last silently decided what the other one's prompt said.
pub const SUFFIX: &str = "_inv";

/// Every language the game ships a message archive for.
pub const LANGUAGES: [&str; 8] = ["chS", "chT", "eng", "fre", "ger", "ita", "jpn", "spa"];

pub fn original(dir: &Path, language: &str) -> PathBuf {
    dir.join(format!("msg_{language}.arc"))
}

pub fn patched_path(dir: &Path, language: &str) -> PathBuf {
    dir.join(format!("msg_{language}{SUFFIX}.arc"))
}

pub fn open(path: &Path) -> Result<arc::Archive, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    arc::Archive::read(&bytes)
}

/// Reads every archive and rebuilds every message file inside it, unchanged.
///
/// This proves the format is understood before anything is written. A message
/// file that comes back different is one this crate must not be editing.
/// Returns how many files were checked, or the first group of failures.
pub fn verify(dir: &Path) -> Result<usize, String> {
    let mut checked = 0;
    let mut failures = Vec::new();

    for language in LANGUAGES {
        let path = original(dir, language);
        if !path.exists() {
            continue;
        }

        let archive = open(&path)?;

        for entry in &archive.entries {
            if !entry.name.starts_with("message\\") {
                continue;
            }

            checked += 1;
            if let Err(e) = gmd::verify(&entry.data) {
                failures.push(format!("{language} {}: {e}", entry.name));
            }
        }
    }

    if failures.is_empty() {
        Ok(checked)
    } else {
        Err(failures.join("; "))
    }
}

/// Builds one language's patched archive beside the original.
///
/// Returns a short description of what changed in the prompt text.
pub fn build_one(dir: &Path, language: &str) -> Result<String, String> {
    let source = original(dir, language);
    let mut archive = open(&source)?;

    let name = format!("message\\message_commonmsg_{language}");
    let entry = archive
        .find(&name)
        .ok_or_else(|| format!("no entry named '{name}'"))?;

    let mut file = gmd::Gmd::read(&entry.data)?;
    let mut changed = Vec::new();

    let current = file
        .get(typewriter::WITH_RIBBON)
        .ok_or("the message file is shorter than expected")?;

    match typewriter::with_ribbon(&current) {
        Some(patched) => {
            file.set(typewriter::WITH_RIBBON, &patched)?;
            changed.push("with a ribbon");
        }
        None => return Err("the prompt is not the one expected".into()),
    }

    let current = file
        .get(typewriter::WITHOUT_RIBBON)
        .ok_or("the message file is shorter than expected")?;

    match typewriter::without_ribbon(&current, language) {
        Some(patched) => {
            file.set(typewriter::WITHOUT_RIBBON, &patched)?;
            changed.push("without one");
        }
        None if !typewriter::translated(language) => changed.push("not translated"),
        None => changed.push("already offers a choice"),
    }

    entry.data = file.write();

    let out = patched_path(dir, language);
    std::fs::write(&out, archive.write()).map_err(|e| format!("{}: {e}", out.display()))?;

    Ok(changed.join(", "))
}

/// Builds every language whose patched archive is missing.
///
/// This is what the mod runs at startup, so installing it is nothing but
/// copying files: the first launch writes the archives, later launches find
/// them present and write nothing. The player's own archives are only ever
/// read.
///
/// Verification runs first, once, and refuses everything if any message file
/// does not rebuild byte-identical: a format this crate has misread is one it
/// has no business editing.
///
/// Returns `(language, outcome)` for each archive written.
pub fn build_missing(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let missing: Vec<&str> = LANGUAGES
        .iter()
        .filter(|language| original(dir, language).exists())
        .filter(|language| !patched_path(dir, language).exists())
        .copied()
        .collect();

    if missing.is_empty() {
        return Ok(Vec::new());
    }

    verify(dir)?;

    let mut written = Vec::new();
    for language in missing {
        match build_one(dir, language) {
            Ok(report) => written.push((language.to_string(), report)),
            Err(e) => written.push((language.to_string(), format!("skipped: {e}"))),
        }
    }

    Ok(written)
}
