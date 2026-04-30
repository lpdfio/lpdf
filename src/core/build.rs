//! Build script — injects the trusted Ed25519 public key(s) at compile time.
//!
//! Reads the `LPDF_PUBLIC_KEY` environment variable, which must contain one or
//! more comma-separated 64-character lowercase hex strings (one per Ed25519
//! public key), and writes `$OUT_DIR/trusted_keys.rs`:
//!
//! ```rust
//! pub const TRUSTED_KEYS: &[[u8; 32]] = &[
//!     [0xab, 0xcd, ...],
//!     // ...
//! ];
//! ```
//!
//! This file is `include!`d by `src/license.rs`, keeping the key out of
//! version control entirely.
//!
//! # Multiple keys / rotation
//!
//! Separate keys with commas: `LPDF_PUBLIC_KEY=<key1>,<key2>`.
//! A token is accepted if it verifies against any key in the list.
//! During rotation: prepend the new key, redeploy, reissue tokens, then
//! remove the old key after the grace period and redeploy again.
//!
//! # Local development
//!
//! Run `npm start` inside `src/license/` once to generate a dev keypair.
//! The server prints a hex string — set it in your shell:
//!
//!   export LPDF_PUBLIC_KEY=$(cat src/license/keys/public.hex)
//!
//! # CI / GitHub Actions
//!
//! Store the production public key as a repository secret named
//! `LPDF_PUBLIC_KEY` and add the following to every `cargo` step:
//!
//!   env:
//!     LPDF_PUBLIC_KEY: ${{ secrets.LPDF_PUBLIC_KEY }}

use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-env-changed=LPDF_PUBLIC_KEY");

    // If LPDF_PUBLIC_KEY is not set, emit an empty key slice.  License
    // verification will fail at runtime when a token is supplied, but binaries
    // that never call render (e.g. codegen) can be built without the key.
    let raw = env::var("LPDF_PUBLIC_KEY").unwrap_or_default();
    let raw = raw.trim();

    // Parse comma-separated hex keys (skip if empty).
    let keys: Vec<[u8; 32]> = if raw.is_empty() {
        vec![]
    } else {
        raw.split(',')
            .enumerate()
            .map(|(i, s)| {
                let s = s.trim();
                assert!(
                    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()),
                    "LPDF_PUBLIC_KEY entry {} must be a 64-character lowercase hex string (got {:?})",
                    i + 1,
                    s
                );
                let mut out = [0u8; 32];
                for (j, byte) in out.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&s[j * 2..j * 2 + 2], 16)
                        .expect("invalid hex digit");
                }
                out
            })
            .collect()
    };

    // Emit the Rust source file.
    let mut src = String::from("pub const TRUSTED_KEYS: &[[u8; 32]] = &[\n");
    for key in &keys {
        src.push_str("    [");
        for (i, b) in key.iter().enumerate() {
            if i > 0 {
                src.push_str(", ");
            }
            src.push_str(&format!("0x{b:02x}"));
        }
        src.push_str("],\n");
    }
    src.push_str("];\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set by cargo");
    fs::write(Path::new(&out_dir).join("trusted_keys.rs"), src)
        .expect("failed to write trusted_keys.rs");
}
