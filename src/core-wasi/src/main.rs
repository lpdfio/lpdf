//! The WASI binary of lpdf: reads a request in JSON on stdin and writes its answer to stdout. The protocol is in
//! `dispatch.rs`, which the native library (`src/core-ffi`) shares.

include!("core_modules.rs");

mod dispatch;

use std::io::{Read, Write};

/// Counts what each render allocates, for its memory cap.
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;

/// The most of the answer held for stdout at once, in bytes: the encoder hands it up to 1 KB at a time, and each write
/// to stdout is a call to the host.
const STDOUT_BUFFER: usize = 64 * 1024;

fn main() {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).unwrap_or(0);
    let answer = dispatch::answer(buf);
    let mut stdout = std::io::BufWriter::with_capacity(answer.json_len().min(STDOUT_BUFFER), std::io::stdout().lock());
    answer.write_to(&mut stdout).and_then(|()| stdout.flush()).expect("stdout takes the answer");
}
