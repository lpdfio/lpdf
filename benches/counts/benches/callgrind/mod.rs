//! The Callgrind setup of the instruction-count binaries, `counts.rs`, `fonts.rs` and `data.rs`.

use gungraun::LibraryBenchmarkConfig;

/// glibc's malloc, which Callgrind counts, grows its heap in steps that depend on the address the heap
/// starts at, and so on the size of the binary: a large block is carved from the heap or mapped on its
/// own depending on whether the heap has room left, and frees then merge differently. Any change to the
/// code moved small renders by up to 1% that way. One large heap from the start, never given back,
/// leaves the allocator with the same work for the same calls.
const GLIBC_TUNABLES: &str =
    "glibc.malloc.top_pad=268435456:glibc.malloc.trim_threshold=4294967296";

/// Callgrind only. DHAT under Gungraun keeps an allocation only when the benchmark's frame is among the
/// innermost of its stack, 12 by default, so it misses nearly all of layout's: `allocations.rs` counts
/// them. `scripts/bench-counts.sh` runs these binaries under DHAT once more, with whole stacks, for the
/// profiles only.
pub fn config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.env("GLIBC_TUNABLES", GLIBC_TUNABLES);
    config
}
