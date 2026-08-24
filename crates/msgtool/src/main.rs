//! Reads and rebuilds the game's message archives.
//!
//! The item box needs a third choice on the typewriter prompt, and what the
//! prompt offers is decided by the game's message files rather than by its code.
//! So there is no way to add one without touching that text.
//!
//! What this tool never does is modify the player's own files. It reads
//! `msg_<lang>.arc`, edits a copy in memory, and writes the result out under a
//! different name that the mod redirects to at runtime. The originals stay
//! exactly as Steam installed them, so verifying the game files reports nothing
//! and uninstalling is a delete.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod arc;
mod gmd;
mod typewriter;

/// Suffix for the archives this tool writes.
///
/// Deliberately not `_box`, which is what re0box writes. Both mods redirect the
/// game to their own copy of the same file, and sharing a name would mean
/// whichever loaded last silently decided what the other one's prompt said.
const SUFFIX: &str = "_inv";

/// Every language the game ships a message archive for.
const LANGUAGES: [&str; 8] = ["chS", "chT", "eng", "fre", "ger", "ita", "jpn", "spa"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    match args {
        ["list", path] => list(Path::new(path)),
        ["show", path, entry] => show(Path::new(path), entry),
        ["verify", dir] => verify(Path::new(dir)),
        ["build", dir] => build(Path::new(dir)),
        _ => {
            usage();
            Err("no command given".into())
        }
    }
}

fn usage() {
    eprintln!("re0msg <command>");
    eprintln!();
    eprintln!("  list   <arc>            every entry in an archive, with its sizes");
    eprintln!("  show   <arc> <entry>    the strings in a message entry");
    eprintln!("  verify <message-dir>    proves every archive rebuilds unchanged");
    eprintln!("  build  <message-dir>    writes the patched archives next to the originals");
}

fn list(path: &Path) -> Result<(), String> {
    let archive = open(path)?;

    println!("version {}, {} entries", archive.version, archive.entries.len());
    for entry in &archive.entries {
        println!(
            "  {:<48} {:>8} bytes  ext 0x{:08X}",
            entry.name,
            entry.data.len(),
            entry.extension
        );
    }

    Ok(())
}

fn show(path: &Path, name: &str) -> Result<(), String> {
    let mut archive = open(path)?;
    let entry = archive
        .find(name)
        .ok_or_else(|| format!("no entry named '{name}'"))?;

    let total = entry.data.len();
    let file = gmd::Gmd::read(&entry.data)?;
    println!("{}", file.describe(total));

    for (index, text) in file.strings().iter().enumerate() {
        println!("  [{index:>3}] {text:?}");
    }

    Ok(())
}

/// Reads every archive and rebuilds every message file inside it, unchanged.
///
/// This proves the format is understood before anything is written. A message
/// file that comes back different is one this tool must not be editing.
fn verify(dir: &Path) -> Result<(), String> {
    let mut checked = 0;
    let mut failed = 0;

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
                failed += 1;
                println!("  FAIL {language} {}: {e}", entry.name);
            }
        }
    }

    println!("{checked} message file(s) checked, {failed} failed.");

    if failed > 0 {
        return Err("the format is not fully understood; nothing should be written".into());
    }

    Ok(())
}

fn build(dir: &Path) -> Result<(), String> {
    let mut built = 0;

    for language in LANGUAGES {
        let source = original(dir, language);
        if !source.exists() {
            continue;
        }

        match build_one(&source, dir, language) {
            Ok(report) => {
                built += 1;
                println!("  {language}: {report}");
            }
            Err(e) => println!("  {language}: skipped, {e}"),
        }
    }

    if built == 0 {
        return Err(format!("no message archives found in {}", dir.display()));
    }

    println!("{built} archive(s) written.");
    Ok(())
}

fn build_one(source: &Path, dir: &Path, language: &str) -> Result<String, String> {
    let mut archive = open(source)?;

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

    Ok(format!("{} ({})", out.display(), changed.join(", ")))
}

fn original(dir: &Path, language: &str) -> PathBuf {
    dir.join(format!("msg_{language}.arc"))
}

fn patched_path(dir: &Path, language: &str) -> PathBuf {
    dir.join(format!("msg_{language}{SUFFIX}.arc"))
}

fn open(path: &Path) -> Result<arc::Archive, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    arc::Archive::read(&bytes)
}
