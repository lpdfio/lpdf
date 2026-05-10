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

use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-env-changed=LPDF_PUBLIC_KEY");

    let raw = env::var("LPDF_PUBLIC_KEY").unwrap_or_else(|_| {
        // No key set — abort with a clear, actionable message.
        panic!(
            "\n\
            ┌─────────────────────────────────────────────────────────────────┐\n\
            │  LPDF_PUBLIC_KEY is not set.                                    │\n\
            │                                                                 │\n\
            │  For local development:                                         │\n\
            │    cd src/license && npm start                                  │\n\
            │    export LPDF_PUBLIC_KEY=$(cat src/license/keys/public.hex)    │\n\
            │                                                                 │\n\
            │  For CI / GitHub Actions, add to your workflow step:            │\n\
            │    env:                                                         │\n\
            │      LPDF_PUBLIC_KEY: ${{{{ secrets.LPDF_PUBLIC_KEY }}}}         │\n\
            └─────────────────────────────────────────────────────────────────┘"
        )
    });

    // Parse comma-separated hex keys.
    let keys: Vec<([u8; 8], [u8; 32])> = raw
        .split(',')
        .enumerate()
        .map(|(i, s)| {
            let s = s.trim();
            assert!(
                s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()),
                "LPDF_PUBLIC_KEY entry {} must be a 64-character lowercase hex string (got {:?})",
                i + 1,
                s
            );
            let mut key = [0u8; 32];
            for (j, byte) in key.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&s[j * 2..j * 2 + 2], 16)
                    .expect("invalid hex digit");
            }
            let hash = Sha256::digest(key);
            let mut fingerprint = [0u8; 8];
            fingerprint.copy_from_slice(&hash[..8]);
            (fingerprint, key)
        })
        .collect();

    assert!(!keys.is_empty(), "LPDF_PUBLIC_KEY must contain at least one key");

    // Emit the Rust source file.
    let mut src = String::from("pub const TRUSTED_KEYS_WITH_KID: &[([u8; 8], [u8; 32])] = &[\n");
    for (fingerprint, key) in &keys {
        src.push_str("    ([");
        for (i, b) in fingerprint.iter().enumerate() {
            if i > 0 {
                src.push_str(", ");
            }
            src.push_str(&format!("0x{b:02x}"));
        }
        src.push_str("], [");
        for (i, b) in key.iter().enumerate() {
            if i > 0 {
                src.push_str(", ");
            }
            src.push_str(&format!("0x{b:02x}"));
        }
        src.push_str("]),\n");
    }
    src.push_str("];\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set by cargo");
    fs::write(Path::new(&out_dir).join("trusted_keys.rs"), src)
        .expect("failed to write trusted_keys.rs");
}
