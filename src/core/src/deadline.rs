//! The deadline of a render in the native library (`src/core-ffi`).
//!
//! The library runs in its host's own process: the PHP SDK loads it through FFI, where there is no process to end at
//! the timeout and PHP's `max_execution_time` doesn't stop a call into C. So the render stops itself. The request
//! carries the timeout, the render starts its deadline with `start`, and the layout, the data binding and the PDF
//! writer ask `check` as they go: `check_now` once a page, `check` once a node, which reads the clock every
//! `CHECKS_PER_READ` calls. Past the deadline, the check unwinds the render with `resume_unwind`, which skips the panic
//! hook, to the `catch_unwind` of the library's entry point, which answers the timeout's error (`TimedOut`). Each frame
//! frees what it holds on the way out, and what a render keeps for the thread is reset when the next one starts.
//!
//! The WebAssembly builds abort on a panic and can't unwind, so they compile the checks out: their hosts stop a render
//! that runs past its timeout themselves, by ending the process, at an epoch deadline, or by ending the worker.
//!
//! What a check can't stop: a step inside a dependency, such as reading the XML, subsetting a font or compressing an
//! image, runs to its end, and a loop with no check in it runs on.

#[cfg(not(target_family = "wasm"))]
use std::cell::Cell;
#[cfg(not(target_family = "wasm"))]
use std::time::{Duration, Instant};

/// How many calls of `check` pass between two reads of the clock. A read takes tens of nanoseconds and a node of the
/// layout about a microsecond, so the clock costs a render nothing to speak of, and a render runs at most a fraction
/// of a millisecond past its deadline before a check sees it.
#[cfg(not(target_family = "wasm"))]
const CHECKS_PER_READ: u32 = 256;

#[cfg(not(target_family = "wasm"))]
thread_local! {
    /// When the render going on on this thread must stop; `None` outside a render, and for one with no timeout.
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The timeout of the render going on, in seconds, for its error.
    static SECONDS: Cell<f64> = const { Cell::new(0.0) };
    /// The calls of `check` since the clock was last read.
    static CHECKS: Cell<u32> = const { Cell::new(0) };
}

/// What a render past its deadline unwinds with, to the library's entry point: the timeout it had.
#[cfg(not(target_family = "wasm"))]
pub(crate) struct TimedOut {
    /// The timeout, in seconds.
    pub seconds: f64,
}

#[cfg(not(target_family = "wasm"))]
impl TimedOut {
    /// The error of a render that ran past its timeout, which names the timeout.
    pub(crate) fn error(&self) -> String {
        let unit = if self.seconds == 1.0 { "second" } else { "seconds" };
        format!(
            "the render ran past its timeout of {} {unit}: raise the timeout in the engine's options, or render the \
            document in smaller parts",
            self.seconds
        )
    }
}

/// The timeout of a render, from the `timeout` its request gave in seconds: 0, no deadline, when it gave none.
///
/// Returns an error naming what a timeout may be when `seconds` is negative, or not a finite number.
pub(crate) fn seconds(seconds: Option<f64>) -> Result<f64, String> {
    match seconds {
        None => Ok(0.0),
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(seconds),
        Some(seconds) => Err(seconds_error(&seconds.to_string())),
    }
}

/// The error of a timeout given as `shown`, which is not a number of seconds from 0 up.
pub(crate) fn seconds_error(shown: &str) -> String {
    format!("the timeout is a number of seconds, 0 for no limit, not {shown}")
}

/// The deadline of the render going on, until it is dropped.
#[must_use = "the deadline holds only while the guard lives"]
pub(crate) struct Deadline(());

/// Start the deadline of a render that may run `seconds` from now; with 0 it runs with no limit.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn start(seconds: f64) -> Deadline {
    let deadline = (seconds > 0.0)
        .then(|| Duration::try_from_secs_f64(seconds).ok())
        .flatten()
        .and_then(|timeout| Instant::now().checked_add(timeout));
    DEADLINE.set(deadline);
    SECONDS.set(seconds);
    CHECKS.set(0);
    Deadline(())
}

/// The WebAssembly builds keep no deadline: their hosts stop a render themselves.
#[cfg(target_family = "wasm")]
pub(crate) fn start(_seconds: f64) -> Deadline {
    Deadline(())
}

#[cfg(not(target_family = "wasm"))]
impl Drop for Deadline {
    fn drop(&mut self) {
        DEADLINE.set(None);
    }
}

/// Stop the render going on, by unwinding it to the library's entry point, once it is past its deadline. Reads the
/// clock every `CHECKS_PER_READ` calls: for a step that comes often, such as each node of the layout.
#[cfg(not(target_family = "wasm"))]
#[inline]
pub(crate) fn check() {
    let checks = CHECKS.get() + 1;
    if checks < CHECKS_PER_READ {
        CHECKS.set(checks);
        return;
    }
    CHECKS.set(0);
    check_now();
}

/// Stop the render going on once it is past its deadline, reading the clock now: for a step that comes once a page.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn check_now() {
    if let Some(deadline) = DEADLINE.get() {
        if Instant::now() >= deadline {
            // Cleared here, so a check in a frame that is dropped on the way out doesn't unwind a second time.
            DEADLINE.set(None);
            std::panic::resume_unwind(Box::new(TimedOut { seconds: SECONDS.get() }));
        }
    }
}

/// Compiled out of the WebAssembly builds, which abort on a panic.
#[cfg(target_family = "wasm")]
#[inline(always)]
pub(crate) fn check() {}

/// Compiled out of the WebAssembly builds, which abort on a panic.
#[cfg(target_family = "wasm")]
#[inline(always)]
pub(crate) fn check_now() {}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    /// What `run` unwound with, if it was a render past its deadline.
    fn timed_out(run: impl FnOnce()) -> Option<f64> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
            .err()
            .and_then(|payload| payload.downcast::<TimedOut>().ok())
            .map(|timed_out| timed_out.seconds)
    }

    #[test]
    fn a_check_past_the_deadline_unwinds_with_the_timeout() {
        let seconds = timed_out(|| {
            let _deadline = start(0.001);
            std::thread::sleep(Duration::from_millis(5));
            check_now();
        });
        assert_eq!(seconds, Some(0.001));
    }

    #[test]
    fn a_counted_check_reads_the_clock_every_so_many_calls() {
        let seconds = timed_out(|| {
            let _deadline = start(0.001);
            std::thread::sleep(Duration::from_millis(5));
            for _ in 1..CHECKS_PER_READ {
                check();
            }
            assert!(DEADLINE.get().is_some(), "no read of the clock before the last call");
            check();
        });
        assert_eq!(seconds, Some(0.001));
    }

    #[test]
    fn a_render_with_no_timeout_and_the_one_after_a_render_keep_no_deadline() {
        let _deadline = start(0.0);
        check_now();
        drop(_deadline);
        {
            let _deadline = start(0.001);
        }
        std::thread::sleep(Duration::from_millis(5));
        check_now();
    }

    #[test]
    fn the_error_names_the_timeout() {
        assert!(TimedOut { seconds: 5.0 }.error().starts_with("the render ran past its timeout of 5 seconds: "));
        assert!(TimedOut { seconds: 1.0 }.error().starts_with("the render ran past its timeout of 1 second: "));
        assert!(TimedOut { seconds: 0.25 }.error().starts_with("the render ran past its timeout of 0.25 seconds: "));
    }

    #[test]
    fn the_timeout_is_a_number_of_seconds_from_0_up() {
        assert_eq!(seconds(None), Ok(0.0));
        assert_eq!(seconds(Some(0.0)), Ok(0.0));
        assert_eq!(seconds(Some(2.5)), Ok(2.5));
        for (value, shown) in [(-1.0, "-1"), (f64::NAN, "NaN"), (f64::INFINITY, "inf")] {
            let error = seconds(Some(value)).unwrap_err();
            assert!(error.ends_with(&format!("0 for no limit, not {shown}")), "{error}");
        }
    }
}
