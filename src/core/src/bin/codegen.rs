//! # lpdf-codegen CLI
//!
//! Reads an LPDF XML file and emits SDK source code for the requested target.
//!
//! ## Usage
//!
//! ```sh
//! cargo run --manifest-path src/core/Cargo.toml --bin codegen -- \
//!     --input invoice.xml --target js --output ./out/invoice.ts
//! ```

use std::path::PathBuf;
use std::process;

fn usage() -> ! {
    eprintln!("Usage: lpdf-codegen --input <file.xml> --target <js> [--output <out>] [--indent 2|4]");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input:  Option<PathBuf> = None;
    let mut target: Option<String>  = None;
    let mut output: Option<PathBuf> = None;
    let mut indent: u8 = 4;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input"  => { i += 1; input  = Some(PathBuf::from(args.get(i).unwrap_or_else(|| usage()))); }
            "--target" => { i += 1; target = Some(args.get(i).unwrap_or_else(|| usage()).clone()); }
            "--output" => { i += 1; output = Some(PathBuf::from(args.get(i).unwrap_or_else(|| usage()))); }
            "--indent" => {
                i += 1;
                indent = args.get(i).unwrap_or_else(|| usage())
                    .parse::<u8>()
                    .unwrap_or_else(|_| { eprintln!("--indent must be 2 or 4"); process::exit(1); });
            }
            "--validate" => { /* no-op placeholder */ }
            other => { eprintln!("Unknown argument: {other}"); usage(); }
        }
        i += 1;
    }

    let input_path  = input.unwrap_or_else(|| { eprintln!("--input is required"); usage(); });
    let target_name = target.unwrap_or_else(|| { eprintln!("--target is required"); usage(); });

    let xml = std::fs::read_to_string(&input_path).unwrap_or_else(|e| {
        eprintln!("Failed to read '{}': {e}", input_path.display());
        process::exit(1);
    });

    let opts = lpdf::codegen::CodegenOptions { target: target_name, indent };

    let source = lpdf::codegen::codegen(&xml, &opts).unwrap_or_else(|e| {
        eprintln!("Codegen error: {e}");
        process::exit(1);
    });

    match output {
        Some(out_path) => {
            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                        eprintln!("Failed to create output directory: {e}");
                        process::exit(1);
                    });
                }
            }
            std::fs::write(&out_path, &source).unwrap_or_else(|e| {
                eprintln!("Failed to write '{}': {e}", out_path.display());
                process::exit(1);
            });
            eprintln!("Written: {}", out_path.display());
        }
        None => print!("{source}"),
    }
}
