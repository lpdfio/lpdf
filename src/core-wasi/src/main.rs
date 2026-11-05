//! The WASI binary of lpdf: reads a request in JSON on stdin and writes its answer to stdout. The protocol is in
//! `dispatch.rs`, which the native library (`src/core-ffi`) shares.

include!("core_modules.rs");

mod dispatch;

use std::io::Read;

/// Counts what each render allocates, for its memory cap.
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;

fn main() {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).unwrap_or(0);
    print!("{}", dispatch::answer(buf));
}
