//! What the two benchmark binaries share: the fixtures, the setup of each stage, and the seed of std's
//! `HashMap`.

use std::path::PathBuf;

use lpdf::{BenchDoc, bench_parse};
#[cfg(feature = "split-stages")]
use lpdf::{BenchPages, bench_layout_doc};

/// std seeds every `HashMap` with bytes from `getrandom`, so each run would hash in another order and
/// count up to 1% more or fewer instructions for the same build. std reaches `getrandom` through a weak
/// symbol, so this definition takes its place in each binary: the same bytes in every run, and the same
/// numbers. It changes nothing in the code measured, only the seed it is given.
///
/// # Safety
///
/// `buffer` must be valid for writes of `length` bytes, as `getrandom(2)` requires of its callers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrandom(buffer: *mut u8, length: usize, _flags: u32) -> isize {
    // SAFETY: the caller passes a buffer of `length` writable bytes, as for the system call.
    unsafe { std::ptr::write_bytes(buffer, 0x5a, length) };
    length as isize
}

/// The text of a fixture of the checkout this crate was copied into.
pub fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures")
        .join(format!("{name}.xml"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the fixture {} can't be read: {e}", path.display()))
}

pub fn parsed(name: &str) -> BenchDoc {
    bench_parse(&fixture(name)).unwrap_or_else(|e| panic!("{name} doesn't parse: {e}"))
}

#[cfg(feature = "split-stages")]
pub fn laid_out(name: &str) -> BenchPages {
    bench_layout_doc(parsed(name))
}
