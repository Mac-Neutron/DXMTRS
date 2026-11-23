//! Offline disassembler for DXSO bytecode dumped via `MTLD3D_BYTECODE_DUMP`.
//!
//! Usage: `cargo run --example disasm --target x86_64-apple-darwin -- <file.dxso>`
//!
//! Prints the parsed IR as Debug so a captured bytecode blob can be inspected
//! without launching Wine.

use std::{env, fs, process};

use mtld3d_core::dxso;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: disasm <file.dxso>");
        process::exit(2);
    });
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            process::exit(2);
        }
    };
    if !bytes.len().is_multiple_of(4) {
        eprintln!(
            "file size {} not a multiple of 4: not a DXSO token stream",
            bytes.len()
        );
        process::exit(2);
    }
    let tokens: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();

    println!("== tokens ({n} words) ==", n = tokens.len());
    for (i, t) in tokens.iter().enumerate() {
        println!("  [{i:3}] {t:#010x}");
    }
    println!();

    let program = match dxso::parse(&tokens) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse failed: {e:?}");
            process::exit(1);
        }
    };
    println!("== parsed IR ==");
    println!("shader_type: {:?}", program.shader_type);
    println!("declarations:");
    for d in &program.declarations {
        println!("  {d:?}");
    }
    println!("def constants:");
    for c in &program.def_constants {
        println!("  {c:?}");
    }
    println!("instructions:");
    for (i, inst) in program.instructions.iter().enumerate() {
        println!("  [{i:3}] {inst:#?}");
    }
}
