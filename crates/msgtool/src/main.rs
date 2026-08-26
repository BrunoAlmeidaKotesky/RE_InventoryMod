//! Command-line front end over the msgtool library.
//!
//! The mod builds the archives itself at startup; this binary exists for
//! inspecting archives and for building them by hand while developing.

use std::path::Path;
use std::process::ExitCode;

use msgtool::{gmd, open, original, LANGUAGES};

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

fn verify(dir: &Path) -> Result<(), String> {
    let checked = msgtool::verify(dir)?;
    println!("{checked} message file(s) checked, 0 failed.");
    Ok(())
}

/// Builds every language present, overwriting patched archives already there.
///
/// Overwriting is the difference from what the mod does at startup: by hand,
/// "build" means "build again".
fn build(dir: &Path) -> Result<(), String> {
    let mut built = 0;

    for language in LANGUAGES {
        if !original(dir, language).exists() {
            continue;
        }

        match msgtool::build_one(dir, language) {
            Ok(report) => {
                built += 1;
                println!(
                    "  {language}: {} ({report})",
                    msgtool::patched_path(dir, language).display()
                );
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
