use std::path::PathBuf;
use std::process;
use std::time::Instant;

/// On Windows, the CRT keeps stdout in text mode by default, which mangles
/// binary output. Switch to binary mode via the CRT's `_setmode` before
/// writing any raw bytes. No-op on every other platform.
fn set_stdout_binary() {
    #[cfg(windows)]
    {
        unsafe extern "C" { fn _setmode(fd: i32, mode: i32) -> i32; }
        const STDOUT_FD: i32 = 1;
        const O_BINARY: i32  = 0x8000;
        // SAFETY: standard MSVCRT call with well-known constant arguments.
        unsafe { _setmode(STDOUT_FD, O_BINARY); }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("codegen")   => cmd_codegen(&args[2..]),
        Some("convert")   => cmd_convert(&args[2..]),
        Some("benchmark") => cmd_benchmark(&args[2..]),
        Some("--help") | Some("-h") => print_top_help(),
        Some(other) => {
            eprintln!("Unknown subcommand: {other}");
            print_top_help();
            process::exit(1);
        }
        None => {
            print_top_help();
            process::exit(1);
        }
    }
}

// ── Subcommand: codegen ───────────────────────────────────────────────────────

fn cmd_codegen(args: &[String]) {
    let mut input:  Option<PathBuf> = None;
    let mut target: Option<String>  = None;
    let mut output: Option<PathBuf> = None;
    let mut indent: u8 = 4;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_codegen_help();
                return;
            }
            "--input"  => { i += 1; input  = Some(PathBuf::from(require_arg(args, i, "--input"))); }
            "--target" => { i += 1; target = Some(require_arg(args, i, "--target").to_owned()); }
            "--output" => { i += 1; output = Some(PathBuf::from(require_arg(args, i, "--output"))); }
            "--indent" => {
                i += 1;
                indent = require_arg(args, i, "--indent")
                    .parse::<u8>()
                    .unwrap_or_else(|_| { eprintln!("--indent must be 2 or 4"); process::exit(1); });
            }
            other => { eprintln!("Unknown argument: {other}"); process::exit(1); }
        }
        i += 1;
    }

    let input_path  = input .unwrap_or_else(|| { eprintln!("--input is required");  process::exit(1); });
    let target_name = target.unwrap_or_else(|| { eprintln!("--target is required"); process::exit(1); });

    let xml = read_file(&input_path);
    let opts = lpdf::codegen::CodegenOptions { target: target_name, indent };

    let t0 = Instant::now();
    let source = lpdf::codegen::codegen(&xml, &opts).unwrap_or_else(|e| {
        eprintln!("Codegen error: {e}");
        process::exit(1);
    });
    let elapsed = t0.elapsed().as_secs_f64();

    let out_path = match output {
        Some(ref p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                        eprintln!("Failed to create output directory: {e}");
                        process::exit(1);
                    });
                }
            }
            std::fs::write(p, source.as_bytes()).unwrap_or_else(|e| {
                eprintln!("Failed to write '{}': {e}", p.display());
                process::exit(1);
            });
            print_output(&to_relative(p), elapsed, source.len(), None);
        }
        None => print!("{source}"),
    };
}

// ── Subcommand: convert ───────────────────────────────────────────────────────

fn cmd_convert(args: &[String]) {
    let mut input:   Option<PathBuf> = None;
    let mut output:  Option<PathBuf> = None;
    let mut license: String          = std::env::var("LPDF_LICENSE").unwrap_or_default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_convert_help();
                return;
            }
            "--input"   => { i += 1; input   = Some(PathBuf::from(require_arg(args, i, "--input"))); }
            "--output"  => { i += 1; output  = Some(PathBuf::from(require_arg(args, i, "--output"))); }
            "--license" => { i += 1; license = require_arg(args, i, "--license").to_owned(); }
            other => { eprintln!("Unknown argument: {other}"); process::exit(1); }
        }
        i += 1;
    }

    let input_path = input.unwrap_or_else(|| { eprintln!("--input is required"); process::exit(1); });

    // ── Folder batch mode ─────────────────────────────────────────────────────
    if input_path.is_dir() {
        let output_dir = output.unwrap_or_else(|| {
            eprintln!("--output <dir> is required when --input is a folder");
            process::exit(1);
        });
        cmd_convert_batch(&input_path, &output_dir, &license);
        return;
    }

    // ── Single file mode ──────────────────────────────────────────────────────
    let xml = read_file(&input_path);

    let t0 = Instant::now();
    let pdf_bytes = lpdf::LpdfEngine::render_xml_to_pdf(&xml, &license).unwrap_or_else(|e| {
        eprintln!("Render error: {e}");
        process::exit(1);
    });
    let elapsed = t0.elapsed().as_secs_f64();

    let pages = count_pdf_pages(&pdf_bytes);

    match output {
        Some(ref p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                        eprintln!("Failed to create output directory: {e}");
                        process::exit(1);
                    });
                }
            }
            std::fs::write(p, &pdf_bytes).unwrap_or_else(|e| {
                eprintln!("Failed to write '{}': {e}", p.display());
                process::exit(1);
            });
            print_output(&to_relative(p), elapsed, pdf_bytes.len(), Some(pages));
        }
        None => {
            use std::io::IsTerminal;
            if std::io::stdout().is_terminal() {
                eprintln!("stdout is a terminal — use --output <file.pdf> to write to a file");
                process::exit(1);
            }
            set_stdout_binary();
            std::io::Write::write_all(&mut std::io::stdout(), &pdf_bytes).unwrap_or_else(|e| {
                eprintln!("Failed to write to stdout: {e}");
                process::exit(1);
            });
        }
    }
}

fn cmd_convert_batch(input_dir: &PathBuf, output_dir: &PathBuf, license: &str) {
    std::fs::create_dir_all(output_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create output directory '{}': {e}", output_dir.display());
        process::exit(1);
    });

    let mut entries: Vec<PathBuf> = std::fs::read_dir(input_dir)
        .unwrap_or_else(|e| { eprintln!("Failed to read directory '{}': {e}", input_dir.display()); process::exit(1); })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "xml").unwrap_or(false))
        .collect();
    entries.sort();

    if entries.is_empty() {
        eprintln!("No .xml files found in '{}'", input_dir.display());
        process::exit(1);
    }

    for xml_path in &entries {
        let stem = {
            let name = xml_path.file_name().unwrap_or_default().to_string_lossy();
            let s = name.strip_suffix(".xml").unwrap_or(&name);
            let s = s.strip_suffix(".lpdf").unwrap_or(s);
            s.to_string()
        };
        let out_path = output_dir.join(format!("{stem}.pdf"));

        let xml = match std::fs::read_to_string(xml_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("Skipping '{}': {e}", xml_path.display()); continue; }
        };

        let t0 = Instant::now();
        let pdf_bytes = match lpdf::LpdfEngine::render_xml_to_pdf(&xml, license) {
            Ok(b) => b,
            Err(e) => { eprintln!("Skipping '{}': {e}", xml_path.display()); continue; }
        };
        let elapsed = t0.elapsed().as_secs_f64();

        if let Err(e) = std::fs::write(&out_path, &pdf_bytes) {
            eprintln!("Failed to write '{}': {e}", out_path.display());
            continue;
        }

        let pages = count_pdf_pages(&pdf_bytes);
        print_output(&to_relative(&out_path), elapsed, pdf_bytes.len(), Some(pages));
    }
}

// ── Output helpers ────────────────────────────────────────────────────────────

/// Format a path relative to cwd for display. Strips the Windows `\\?\`
/// extended-length prefix that `canonicalize()` adds.
fn to_relative(path: &PathBuf) -> String {
    let s = path.display().to_string();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_s = cwd.display().to_string();
        if let Some(rel) = s.strip_prefix(&cwd_s) {
            let rel = rel.trim_start_matches(['\\', '/']);
            if !rel.is_empty() { return rel.to_string(); }
        }
    }
    s.to_string()
}

/// Print one output line with fixed-width time and size columns:
/// `  0.00575s    5.7 KB   output/file.pdf (4 pages)`
fn print_output(rel_path: &str, elapsed_secs: f64, size_bytes: usize, page_count: Option<usize>) {
    let time_str = format!("{:.5}s", elapsed_secs);
    let size_str = format_size(size_bytes);
    match page_count {
        Some(n) => {
            let label = if n == 1 { "page" } else { "pages" };
            println!("{:>10}  {:>9}   {} ({} {})", time_str, size_str, rel_path, n, label);
        }
        None => println!("{:>10}  {:>9}   {}", time_str, size_str, rel_path),
    }
}

fn format_size(bytes: usize) -> String {
    const KB: usize = 1_024;
    const MB: usize = 1_024 * 1_024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Count PDF pages by scanning for `/Type /Page` object entries.
/// Matches `/Type /Page` followed by anything other than `s` so that
/// `/Type /Pages` (the page tree node) is not counted.
fn count_pdf_pages(bytes: &[u8]) -> usize {
    let needle = b"/Type /Page";
    let mut count = 0;
    let mut start = 0;
    while start + needle.len() < bytes.len() {
        if let Some(rel) = bytes[start..].windows(needle.len()).position(|w| w == needle) {
            let abs = start + rel;
            let after = abs + needle.len();
            // /Pages has 's' immediately after; skip it
            if bytes.get(after) != Some(&b's') {
                count += 1;
            }
            start = abs + 1;
        } else {
            break;
        }
    }
    count
}

// ── Misc helpers ──────────────────────────────────────────────────────────────

fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read '{}': {e}", path.display());
        process::exit(1);
    })
}

fn require_arg<'a>(args: &'a [String], i: usize, flag: &str) -> &'a str {
    args.get(i).map(String::as_str).unwrap_or_else(|| {
        eprintln!("{flag} requires a value");
        process::exit(1);
    })
}

// ── Subcommand: benchmark ────────────────────────────────────────────────────

fn cmd_benchmark(args: &[String]) {
    let mut input:  Option<PathBuf> = None;
    let mut repeat: u32             = 100;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => { print_benchmark_help(); return; }
            "--input"  => { i += 1; input  = Some(PathBuf::from(require_arg(args, i, "--input"))); }
            "--repeat" => {
                i += 1;
                repeat = require_arg(args, i, "--repeat")
                    .parse::<u32>()
                    .unwrap_or_else(|_| { eprintln!("--repeat must be a positive integer"); process::exit(1); });
            }
            other => { eprintln!("Unknown argument: {other}"); process::exit(1); }
        }
        i += 1;
    }

    let input_path = input.unwrap_or_else(|| { eprintln!("--input is required"); process::exit(1); });

    let mut entries: Vec<PathBuf> = if input_path.is_dir() {
        std::fs::read_dir(&input_path)
            .unwrap_or_else(|e| { eprintln!("Failed to read directory '{}': {e}", input_path.display()); process::exit(1); })
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "xml").unwrap_or(false))
            .collect()
    } else {
        vec![input_path]
    };
    entries.sort();

    if entries.is_empty() {
        eprintln!("No .xml files found");
        process::exit(1);
    }

    // Header
    eprintln!("Repeats: {repeat}");
    eprintln!("{:>10}  {:>10}  {:>10}  {:>9}   file", "min", "avg", "max", "size");
    eprintln!("{}", "-".repeat(70));

    for xml_path in &entries {
        let xml = match std::fs::read_to_string(xml_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("Skipping '{}': {e}", xml_path.display()); continue; }
        };

        let mut times: Vec<f64> = Vec::with_capacity(repeat as usize);
        let mut last_size = 0usize;

        for _ in 0..repeat {
            let t0 = Instant::now();
            match lpdf::LpdfEngine::render_xml_to_pdf(&xml, "") {
                Ok(b) => { last_size = b.len(); }
                Err(e) => { eprintln!("Error in '{}': {e}", xml_path.display()); break; }
            }
            times.push(t0.elapsed().as_secs_f64());
        }

        if times.is_empty() { continue; }

        let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = times.iter().sum::<f64>() / times.len() as f64;

        let stem = {
            let name = xml_path.file_name().unwrap_or_default().to_string_lossy();
            let s = name.strip_suffix(".xml").unwrap_or(&name);
            s.strip_suffix(".lpdf").unwrap_or(s).to_string()
        };

        println!("{:>10}  {:>10}  {:>10}  {:>9}   {}",
            format!("{:.5}s", min),
            format!("{:.5}s", avg),
            format!("{:.5}s", max),
            format_size(last_size),
            stem,
        );
    }
}

// ── Help text ─────────────────────────────────────────────────────────────────

fn print_top_help() {
    eprintln!("Usage: lpdf <subcommand> [options]");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  codegen     Generate SDK source code from an LPDF XML file");
    eprintln!("  convert     Render an LPDF XML file to PDF");
    eprintln!("  benchmark   Benchmark rendering performance across XML files");
    eprintln!();
    eprintln!("Run `lpdf <subcommand> --help` for subcommand options.");
}

fn print_codegen_help() {
    eprintln!("Usage: lpdf codegen --input <file.xml> --target <lang> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --input  <file.xml>   LPDF XML source (required)");
    eprintln!("  --target <lang>       Output language: js, dotnet, php, python (required)");
    eprintln!("  --output <file>       Write to file instead of stdout");
    eprintln!("  --indent <2|4>        Indentation width (default: 4)");
}

fn print_benchmark_help() {
    eprintln!("Usage: lpdf benchmark --input <file.xml|dir> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --input   <file.xml|dir>   XML file or folder of XML files to benchmark");
    eprintln!("  --repeat  <n>              Number of render iterations per file (default: 100)");
}

fn print_convert_help() {
    eprintln!("Usage: lpdf convert --input <file.xml|dir> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --input   <file.xml|dir>   LPDF XML source file, or folder for batch conversion");
    eprintln!("  --output  <file.pdf|dir>   Write PDF to file instead of stdout; required for folder input");
    eprintln!("  --license <token>          License token (or set LPDF_LICENSE env var)");
}
