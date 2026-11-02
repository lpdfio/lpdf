//! The memory cap of a render.
//!
//! The wasm builds count every allocation with `Counting`, their global allocator. A render starts its cap with
//! `start`, and the parse, the data binding, the layout and the PDF write ask `check` as they go: once the bytes the
//! render holds have gone past the cap, at any moment since it started, the render stops with the cap's error, an
//! ordinary engine error that reads the same in every SDK. What the render holds is counted, not the size of the
//! WebAssembly memory, which never shrinks and stays at its high point after a big render.
//!
//! An allocation is never refused: refused, it would end the engine with a trap. The count only marks the render as
//! past its cap, and the next check stops it. One allocation past the cap and past what WebAssembly can address
//! between two checks still traps, which each SDK reports as the engine crashing.
//!
//! The counts are kept for each thread. The wasm builds have one, and the native tests run on many at once, each
//! render on its own. A native build without `Counting`, such as the command line or the benchmarks, counts nothing,
//! so its renders never reach the cap.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// The cap of a render when the engine's options leave it out, in megabytes.
pub(crate) const DEFAULT_CAP_MB: u32 = 1024;

/// The highest cap, in megabytes: 32-bit WebAssembly addresses 4 GB at most, so a render can't hold more.
pub(crate) const MAX_CAP_MB: u32 = 4096;

const BYTES_PER_MB: i64 = 1_048_576;

thread_local! {
    /// The bytes allocated and not yet freed on this thread.
    static LIVE: Cell<i64> = const { Cell::new(0) };
    /// The `LIVE` past which the render is past its cap; `i64::MAX` outside a render.
    static LIMIT: Cell<i64> = const { Cell::new(i64::MAX) };
    /// Whether `LIVE` went past `LIMIT` since the render started.
    static PASSED: Cell<bool> = const { Cell::new(false) };
    /// The cap of the render going on, in megabytes, for its error.
    static CAP_MB: Cell<u32> = const { Cell::new(DEFAULT_CAP_MB) };
}

/// The system's allocator, counting the bytes it holds for this thread.
// A native build of the core, such as the command line's, keeps its own allocator and leaves this unused.
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
pub(crate) struct Counting;

// SAFETY: every call goes to the system allocator with the caller's arguments; the counting around it touches only
// thread-local cells of plain numbers, which need no allocation and no destructor.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller keeps the contract of `GlobalAlloc::alloc`, which is the system's.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size() as i64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(layout.size() as i64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by this allocator, so by the system's, with `layout`.
        unsafe { System.dealloc(ptr, layout) };
        grow(-(layout.size() as i64));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: as for `dealloc`, and the caller keeps the contract of `GlobalAlloc::realloc`.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            grow(new_size as i64 - layout.size() as i64);
        }
        new_ptr
    }
}

#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
fn grow(bytes: i64) {
    let live = LIVE.get() + bytes;
    LIVE.set(live);
    if live > LIMIT.get() {
        PASSED.set(true);
    }
}

/// The cap of a render, from the `max_memory_mb` an engine was given: `DEFAULT_CAP_MB` when it gave none.
///
/// Returns an error naming the range when `mb` is not a whole number from 1 to `MAX_CAP_MB`.
pub(crate) fn cap_mb(mb: Option<f64>) -> Result<u32, String> {
    match mb {
        None => Ok(DEFAULT_CAP_MB),
        Some(mb) if mb.fract() == 0.0 && (1.0..=f64::from(MAX_CAP_MB)).contains(&mb) => Ok(mb as u32),
        Some(mb) => Err(cap_error(&mb.to_string())),
    }
}

/// The error of a memory cap given as `shown`, which is not a whole number of megabytes from 1 to `MAX_CAP_MB`.
pub(crate) fn cap_error(shown: &str) -> String {
    format!(
        "the memory cap is a whole number of megabytes from 1 to {MAX_CAP_MB}, not {shown}: WebAssembly addresses \
        {MAX_CAP_MB} MB at most"
    )
}

/// The cap of the render going on, until it is dropped.
#[must_use = "the cap holds only while the guard lives"]
pub(crate) struct Cap(());

/// Start the cap of a render: from here, the render may hold `cap_mb` megabytes more than this thread holds now.
pub(crate) fn start(cap_mb: u32) -> Cap {
    CAP_MB.set(cap_mb);
    LIMIT.set(LIVE.get() + i64::from(cap_mb) * BYTES_PER_MB);
    PASSED.set(false);
    Cap(())
}

impl Drop for Cap {
    fn drop(&mut self) {
        LIMIT.set(i64::MAX);
        PASSED.set(false);
    }
}

/// Whether the render going on has held more than its cap.
pub(crate) fn passed() -> bool {
    PASSED.get()
}

/// The cap's error once the render going on has held more than its cap.
pub(crate) fn check() -> Result<(), String> {
    if !passed() {
        return Ok(());
    }
    Err(format!(
        "the render needs more memory than its cap of {} MB: raise the cap in the engine's options, or render the \
        document in smaller parts",
        CAP_MB.get()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_render_that_holds_more_than_its_cap_is_stopped_at_the_next_check() {
        let _cap = start(1);
        assert_eq!(check(), Ok(()));
        let held = vec![0u8; 2 * BYTES_PER_MB as usize];
        let error = check().unwrap_err();
        assert!(error.contains("its cap of 1 MB"), "{error}");
        drop(held);
        assert!(check().is_err(), "the render went past its cap once, which is enough");
    }

    #[test]
    fn what_the_thread_held_before_the_render_does_not_count() {
        let before = vec![0u8; 3 * BYTES_PER_MB as usize];
        let _cap = start(1);
        let small = vec![0u8; 1000];
        assert_eq!(check(), Ok(()));
        drop((before, small));
    }

    #[test]
    fn the_cap_ends_with_its_guard() {
        {
            let _cap = start(1);
            let held = vec![0u8; 2 * BYTES_PER_MB as usize];
            assert!(passed());
            drop(held);
        }
        assert!(!passed());
        let _held = vec![0u8; 2 * BYTES_PER_MB as usize];
        assert_eq!(check(), Ok(()));
    }

    #[test]
    fn the_cap_is_a_whole_number_of_megabytes_from_1_to_4096() {
        assert_eq!(cap_mb(None), Ok(1024));
        assert_eq!(cap_mb(Some(1.0)), Ok(1));
        assert_eq!(cap_mb(Some(4096.0)), Ok(4096));
        for (mb, shown) in [(0.0, "0"), (-1.0, "-1"), (4097.0, "4097"), (1.5, "1.5"), (f64::NAN, "NaN")] {
            let error = cap_mb(Some(mb)).unwrap_err();
            assert!(error.contains("from 1 to 4096") && error.contains(&format!("not {shown}:")), "{error}");
        }
    }
}
