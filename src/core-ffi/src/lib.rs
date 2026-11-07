//! The native library of lpdf, for a host that calls the engine through C in its own process: the PHP SDK, through
//! PHP's FFI, on Linux with glibc. It takes the requests the WASI binary takes and answers them the same way
//! (`dispatch.rs`, which both share), through two functions:
//!
//! ```c
//! uint8_t *lpdf_invoke(const uint8_t *payload, size_t len, size_t *out_len);
//! void lpdf_free(uint8_t *ptr, size_t len);
//! ```
//!
//! What it adds to the WASI binary is the deadline (`deadline.rs`): in the host's process no one can end the render
//! at its `timeout`, so the render stops itself. A render past its deadline, and one that panics, unwinds to
//! `lpdf_invoke` and is answered as an error; it never unwinds into the host. Release builds must keep
//! `panic = "unwind"` for that: with `"abort"`, a timeout or a panic would end the host's process.
//!
//! A stack overflow or a failed allocation can't be caught and ends the host's process. The engine refuses a
//! document nested too deep for the stack before it lays it out (`nesting.rs`), and stops a render past its memory
//! cap (`memory.rs`), which this library counts as the WASI binary does.

include!("../../core-wasi/src/core_modules.rs");

#[path = "../../core-wasi/src/dispatch.rs"]
mod dispatch;

/// Counts what each render allocates, for its memory cap.
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;

/// Answers the request of `len` bytes at `payload`, the JSON the WASI binary reads on stdin, with the JSON it would
/// write to stdout. Returns the answer, `*out_len` bytes of UTF-8, which the caller frees with `lpdf_free`.
///
/// A render past its timeout answers `{ "error": "…", "timed_out": true }`, and one that panics
/// `{ "error": "…", "crashed": true }`.
///
/// # Safety
/// `payload` points to `len` readable bytes, and `out_len` to a writable `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lpdf_invoke(payload: *const u8, len: usize, out_len: *mut usize) -> *mut u8 {
    // SAFETY: the caller passes `len` readable bytes at `payload`.
    let request = unsafe { std::slice::from_raw_parts(payload, len) };
    let answer = match std::str::from_utf8(request) {
        Ok(request) => json_of(dispatch::answer(request.to_string())),
        Err(e) => serde_json::json!({ "error": format!("the request is not UTF-8: {e}") }).to_string().into_bytes(),
    };
    let answer = Box::into_raw(answer.into_boxed_slice());
    // SAFETY: the caller passes a writable `size_t` at `out_len`.
    unsafe { *out_len = answer.len() };
    answer.cast()
}

/// The JSON of `answer`, whole, in a buffer of its length: the host gets the answer in one piece, where the WASI binary
/// writes it to stdout as it encodes it.
fn json_of(answer: dispatch::Answer) -> Vec<u8> {
    let mut json = Vec::with_capacity(answer.json_len());
    match answer.write_to(&mut json) {
        Ok(()) => json,
        // A `Vec` takes all that is written to it, so this is never answered.
        Err(e) => serde_json::json!({ "error": format!("the answer could not be written: {e}") }).to_string().into_bytes(),
    }
}

/// Frees an answer that `lpdf_invoke` returned, of `len` bytes at `ptr`.
///
/// # Safety
/// `ptr` and `len` are an answer and its length that `lpdf_invoke` returned and that was not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lpdf_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        // SAFETY: `ptr` and `len` are the boxed slice `lpdf_invoke` gave up.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

#[cfg(test)]
mod invoke_tests {
    use base64::Engine as _;
    use sha2::Digest as _;

    use super::*;

    /// The text `lpdf_invoke` answers `request`, freed with `lpdf_free` once read.
    fn invoke_text(request: &[u8]) -> String {
        let mut len = 0usize;
        // SAFETY: the request's bytes, and a `usize` to write.
        let answer = unsafe { lpdf_invoke(request.as_ptr(), request.len(), &mut len) };
        // SAFETY: what `lpdf_invoke` returned, `len` bytes, read before it is freed.
        let text = String::from_utf8(unsafe { std::slice::from_raw_parts(answer, len) }.to_vec()).unwrap();
        // SAFETY: the answer and its length, freed once.
        unsafe { lpdf_free(answer, len) };
        text
    }

    /// What `lpdf_invoke` answers `request`.
    fn invoke(request: &[u8]) -> serde_json::Value {
        serde_json::from_str(&invoke_text(request)).unwrap()
    }

    /// The SHA-256 of the PDF in `answer`, in hex.
    fn pdf_hash(answer: &serde_json::Value) -> String {
        let pdf = answer["pdf"].as_str().unwrap_or_else(|| panic!("{answer}"));
        let bytes = base64::engine::general_purpose::STANDARD.decode(pdf).unwrap();
        sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn render(name: &str, xml: &str, timeout: f64) -> serde_json::Value {
        let request = serde_json::json!({ "method": "render_pdf", "input": xml, "key": "test-key", "timeout": timeout });
        let answer = invoke(request.to_string().as_bytes());
        assert!(answer.is_object(), "{name}: {answer}");
        answer
    }

    #[test]
    fn a_render_answers_the_pdf_with_the_stored_hash() {
        let xml = include_str!("../../../test/fixtures/bench_xs.xml");
        let stored = include_str!("../../../test/snapshots/bench_xs.pdf.sha256");
        assert_eq!(pdf_hash(&render("bench_xs", xml, 5.0)), stored.trim());
    }

    #[test]
    fn a_render_answers_the_json_text_that_serde_json_writes_for_its_pdf() {
        let xml = include_str!("../../../test/fixtures/bench_xs.xml");
        let request = serde_json::json!({ "method": "render_pdf", "input": xml });
        let text = invoke_text(request.to_string().as_bytes());
        let answer: serde_json::Value = serde_json::from_str(&text).unwrap();
        let pdf = base64::engine::general_purpose::STANDARD.decode(answer["pdf"].as_str().unwrap()).unwrap();
        // The library answered with serde_json's text of the whole answer before it shared the WASI binary's writer.
        assert!(text == serde_json::json!({ "pdf": base64::engine::general_purpose::STANDARD.encode(&pdf) }).to_string());
    }

    #[test]
    fn a_render_past_its_timeout_is_an_error_and_the_next_gives_the_stored_hash() {
        let xml = include_str!("../../../test/fixtures/bench_xl.xml");
        let stored = include_str!("../../../test/snapshots/bench_xl.pdf.sha256");

        let answer = render("bench_xl", xml, 0.001);
        assert_eq!(answer["timed_out"], true, "{answer}");
        assert!(answer["error"].as_str().unwrap().starts_with("the render ran past its timeout of 0.001 seconds"));

        assert_eq!(pdf_hash(&render("bench_xl", xml, 0.0)), stored.trim());
    }

    #[test]
    fn a_request_that_is_not_utf_8_is_an_error() {
        let answer = invoke(b"{\"method\":\"render_pdf\",\"input\":\"\xff\"}");
        assert!(answer["error"].as_str().unwrap().starts_with("the request is not UTF-8: "), "{answer}");
    }

    #[test]
    fn a_document_nested_past_the_limit_is_the_depth_error() {
        let answer = render("nested", &nesting::nested_xml(101), 5.0);
        let error = answer["error"].as_str().unwrap_or_else(|| panic!("{answer}"));
        assert!(error.contains("is nested 101 levels deep, past the limit of 100"), "{error}");
    }
}
