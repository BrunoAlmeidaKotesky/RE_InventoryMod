//! Static analysis tool for the decrypted RE0 code section.
//!
//! The shipped executable has `.text` encrypted by the Steam DRM stub, so this
//! works on a dump taken from the running process by the mod itself
//! (`DumpText=1` in `re0inv.ini`).

use std::path::PathBuf;
use std::process::ExitCode;

mod bounds;
mod classify;
mod disasm;
mod image;
mod xref;

use image::Image;

/// Runtime virtual address the dumped section started at.
const DEFAULT_BASE: u64 = 0x0040_1000;
const DEFAULT_DUMP: &str = "work/re0hd_text_dump.bin";

const USAGE: &str = "\
re0an - static analysis of the decrypted RE0 code section

USAGE:
    re0an [OPTIONS] <COMMAND> [ARGS]

OPTIONS:
    --dump <PATH>   dump file (default: work/re0hd_text_dump.bin)
    --base <HEX>    virtual address the dump starts at (default: 401000)

COMMANDS:
    disasm <VA> [COUNT]   disassemble COUNT instructions (default 32)
    func <VA>             disassemble until the function appears to end
    xrefs <VA>            find direct calls and jumps targeting VA
    classify <VA>         score every function calling VA for bag scanning
    bounds [N]            find sites indexing an N-element array (default 6)
    datarefs <VA>         find VA appearing as a literal 4-byte value
    context <VA> [COUNT]  disassemble around VA, marking it

Addresses are hexadecimal, with or without a 0x prefix.

EXAMPLES:
    re0an func 4DC8B0
    re0an xrefs 4DC8B0
    re0an datarefs DCBF44
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut dump = PathBuf::from(DEFAULT_DUMP);
    let mut base = DEFAULT_BASE;
    let mut rest: Vec<&str> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dump" => {
                i += 1;
                dump = PathBuf::from(args.get(i).ok_or("--dump needs a path")?);
            }
            "--base" => {
                i += 1;
                base = parse_address(args.get(i).ok_or("--base needs an address")?)?;
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => rest.push(other),
        }
        i += 1;
    }

    let Some((command, operands)) = rest.split_first() else {
        print!("{USAGE}");
        return Ok(());
    };

    let image = Image::load(&dump, base)
        .map_err(|e| format!("could not read {}: {e}", dump.display()))?;

    eprintln!(
        "loaded {} ({} bytes, 0x{:08X} - 0x{:08X})",
        dump.display(),
        image.bytes.len(),
        image.base,
        image.end()
    );

    match *command {
        "disasm" => {
            let va = operand_address(operands, 0, "disasm needs an address")?;
            let count = operand_number(operands, 1).unwrap_or(32);
            disasm::print(&disasm::decode(&image, va, count));
        }

        "func" => {
            let va = operand_address(operands, 0, "func needs an address")?;
            let lines = disasm::function(&image, va);
            disasm::print(&lines);
            eprintln!("{} instructions", lines.len());
        }

        "xrefs" => {
            let va = operand_address(operands, 0, "xrefs needs an address")?;
            report_code_refs(&image, va);
        }

        "classify" => {
            let va = operand_address(operands, 0, "classify needs an address")?;
            classify::report(&image, va);
        }

        "bounds" => {
            let capacity = operand_number(operands, 0).unwrap_or(6) as u8;
            bounds::report(&image, capacity, bounds::DEFAULT_ARRAY_ASSERT_STRING);
        }

        "datarefs" => {
            let va = operand_address(operands, 0, "datarefs needs an address")?;
            let refs = xref::data_refs(&image, va);
            eprintln!("{} literal references to 0x{:08X}", refs.len(), va);
            for r in &refs {
                println!("0x{:08X}", r.from);
            }
        }

        "context" => {
            let va = operand_address(operands, 0, "context needs an address")?;
            let count = operand_number(operands, 1).unwrap_or(24);
            xref::context(&image, va, 24, count);
        }

        other => return Err(format!("unknown command '{other}'")),
    }

    Ok(())
}

fn report_code_refs(image: &Image, target: u64) {
    let refs = xref::code_refs(image, target);

    eprintln!("{} direct references to 0x{:08X}", refs.len(), target);
    eprintln!("(indirect calls through vtables or function pointers are not visible)");

    for r in &refs {
        let kind = match r.kind {
            xref::RefKind::Call => "call",
            xref::RefKind::Jump => "jmp ",
            xref::RefKind::Data => "data",
        };

        match xref::enclosing_function(image, r.from, 0x2000) {
            Some(start) => println!(
                "0x{:08X}  {}  in function 0x{:08X} (+0x{:X})",
                r.from,
                kind,
                start,
                r.from - start
            ),
            None => println!("0x{:08X}  {}  enclosing function unknown", r.from, kind),
        }
    }
}

fn operand_address(operands: &[&str], index: usize, message: &str) -> Result<u64, String> {
    let raw = operands.get(index).ok_or_else(|| message.to_string())?;
    parse_address(raw)
}

fn operand_number(operands: &[&str], index: usize) -> Option<usize> {
    operands.get(index)?.parse().ok()
}

fn parse_address(text: &str) -> Result<u64, String> {
    let cleaned = text.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(cleaned, 16).map_err(|_| format!("'{text}' is not a hex address"))
}
