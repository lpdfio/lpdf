//! License token verification.
//!
//! Token format: `<payload_base64url>.<signature_base64url>`
//! - Payload  : UTF-8 JSON, Base64url-encoded (no padding).
//! - Signature: Ed25519 signature over the raw payload bytes, Base64url-encoded.
//!
//! ## Token claims
//!
//! | Claim  | Type    | Required        | Description                                      |
//! |--------|---------|-----------------|--------------------------------------------------|
//! | `tier` | string  | all tiers       | `"community"`, `"professional"`, `"enterprise"` |
//! | `v`    | integer | all tiers       | Major version the key was issued for             |
//! | `exp`  | integer | community / pro | Unix timestamp; absent on enterprise tokens      |
//! | `kid`  | string  | all tiers       | 16-char hex fingerprint of the signing key       |
//!
//! ## Validation logic
//!
//! 1. Verify Ed25519 signature (using `kid` for direct key lookup if present).
//! 2. Check `v == LPDF_MAJOR_VERSION` — all tiers; mismatch → [`LicenseStatus::VersionMismatch`].
//! 3. If tier is not enterprise, check `exp > now_unix` → [`LicenseStatus::Expired`].
//!
//! Enterprise keys have no date expiry; the ongoing contract is the enforcement
//! mechanism.  All keys become invalid on major version change — the customer
//! generates a new key from the portal as part of their upgrade.
//!
//! ## Signing-key rotation
//!
//! Trusted public keys are embedded in `TRUSTED_KEYS_WITH_KID` below, each
//! paired with its 8-byte SHA-256 fingerprint.  Tokens must include a `kid`
//! claim (16-char lowercase hex of the fingerprint) to identify the signing
//! key; tokens without `kid` are rejected as malformed.
//!
//! Rotation procedure:
//!
//! 1. Generate a new keypair with `npm start` in `src/license/`.
//! 2. Prepend the new hex key to `LPDF_PUBLIC_KEY` (comma-separated).
//! 3. Rebuild and deploy the binary.
//! 4. Reissue tokens to customers (signed with the new private key).
//! 5. After the grace period remove the old key, rebuild, and deploy again.
//!
//! # Local test setup
//! Run `npm start` inside `src/license/` once to auto-generate a keypair.
//! The server prints a hex string — set it as `LPDF_PUBLIC_KEY` in your
//! shell and rebuild.  See `build.rs` for full instructions.
//! The server's `keys/private.hex` is gitignored; never commit it.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey, Verifier};

// ---------------------------------------------------------------------------
// Trusted public keys — injected at compile time via build.rs
// ---------------------------------------------------------------------------
//
// `TRUSTED_KEYS_WITH_KID` is generated from `LPDF_PUBLIC_KEY` by `build.rs`
// and written to `$OUT_DIR/trusted_keys.rs`.  Each entry is a
// `(fingerprint, key_bytes)` tuple where `fingerprint` is SHA-256(key)[..8].
// See `build.rs` for rotation instructions and local development setup.
include!(concat!(env!("OUT_DIR"), "/trusted_keys.rs"));

// Major version embedded at compile time.  A key token's `v` claim must match
// this value or validation fails for all tiers.
// TODO: inject via build.rs reading LPDF_MAJOR_VERSION env var (same pattern as LPDF_PUBLIC_KEY).
const LPDF_MAJOR_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Status type
// ---------------------------------------------------------------------------

/// Result of checking a license token.
#[derive(Debug, PartialEq)]
pub enum LicenseStatus {
    /// Token is valid and not expired.  Inner value is the tier name.
    Licensed(String),
    /// No token was supplied — expected free-mode usage, no warning needed.
    Free,
    /// Token carried a valid signature but has passed its `exp` timestamp.
    Expired,
    /// Token was issued for a different major version of lpdf.
    VersionMismatch,
    /// Token is present but has an invalid Ed25519 signature.
    InvalidSignature,
    /// Token is present but cannot be parsed (bad Base64, bad JSON, missing fields).
    Malformed,
}

impl LicenseStatus {
    /// `true` when the token is valid and within its expiry window.
    pub fn is_licensed(&self) -> bool {
        matches!(self, LicenseStatus::Licensed(_))
    }

    /// Human-readable warning for conditions that indicate a bad token was
    /// supplied.  Returns `None` for expected states (free / expired).
    pub fn warning(&self) -> Option<&'static str> {
        match self {
            LicenseStatus::VersionMismatch => {
                Some("license key was issued for a different major version — generate a new key in the portal")
            }
            LicenseStatus::InvalidSignature => {
                Some("license token has an invalid signature — running in free mode")
            }
            LicenseStatus::Malformed => {
                Some("license token is malformed — running in free mode")
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Check `token` against the embedded public key.
///
/// `now_unix` is the current Unix timestamp in seconds, supplied by the host
/// (WASM/WASI have no system clock).  Pass `0` to skip expiry checking.
///
/// An empty `token` returns [`LicenseStatus::Free`] immediately.  An invalid
/// or malformed token always falls back to free mode (PDF still renders) and
/// carries an optional [`LicenseStatus::warning`] string the caller can
/// surface in the output.
pub fn check(token: &str, now_unix: i64) -> LicenseStatus {
    if token.is_empty() {
        return LicenseStatus::Free;
    }

    // ── Split <payload>.<signature> ──────────────────────────────────────────
    let dot = match token.find('.') {
        Some(i) => i,
        None    => return LicenseStatus::Malformed,
    };
    let payload_b64 = &token[..dot];
    let sig_b64     = &token[dot + 1..];

    if payload_b64.is_empty() || sig_b64.is_empty() {
        return LicenseStatus::Malformed;
    }

    // ── Decode Base64url ─────────────────────────────────────────────────────
    let payload_bytes = match URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(b)  => b,
        Err(_) => return LicenseStatus::Malformed,
    };
    let sig_bytes = match URL_SAFE_NO_PAD.decode(sig_b64) {
        Ok(b)  => b,
        Err(_) => return LicenseStatus::Malformed,
    };

    // ── Parse JSON claims (untrusted — used only for kid-based key selection) ─
    let claims: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v)  => v,
        Err(_) => return LicenseStatus::Malformed,
    };

    // ── Verify Ed25519 signature ─────────────────────────────────────────────
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s)  => s,
        Err(_) => return LicenseStatus::Malformed,
    };

    if TRUSTED_KEYS_WITH_KID.is_empty() {
        return LicenseStatus::InvalidSignature;
    }

    let kid = match claims["kid"].as_str() {
        Some(s) => match parse_kid_hex(s) {
            Some(b) => b,
            None    => return LicenseStatus::Malformed,
        },
        None => return LicenseStatus::Malformed,
    };

    let verified = TRUSTED_KEYS_WITH_KID
        .iter()
        .find(|(fp, _)| *fp == kid)
        .map(|(_, key_bytes)| {
            VerifyingKey::from_bytes(key_bytes)
                .map(|vk| vk.verify(&payload_bytes, &signature).is_ok())
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if !verified {
        return LicenseStatus::InvalidSignature;
    }

    // ── Parse trusted claims (signature verified) ────────────────────────────
    let ver = match claims["v"].as_u64() {
        Some(n) => n as u32,
        None    => return LicenseStatus::Malformed,
    };
    let tier = match claims["tier"].as_str() {
        Some(s) => s.to_string(),
        None    => return LicenseStatus::Malformed,
    };

    // ── Version check — all tiers ────────────────────────────────────────────
    if ver != LPDF_MAJOR_VERSION {
        return LicenseStatus::VersionMismatch;
    }

    // ── Expiry check — Community and Pro only ────────────────────────────────
    // Enterprise keys carry no `exp` claim; the contract governs date-based use.
    if !tier.eq_ignore_ascii_case("enterprise") {
        let exp = match claims["exp"].as_i64() {
            Some(n) => n,
            None    => return LicenseStatus::Malformed,
        };
        if now_unix > 0 && now_unix > exp {
            return LicenseStatus::Expired;
        }
    }

    LicenseStatus::Licensed(tier)
}

fn parse_kid_hex(s: &str) -> Option<[u8; 8]> {
    if s.len() != 16 {
        return None;
    }
    let mut out = [0u8; 8];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_token_is_free() {
        assert_eq!(check("", 0), LicenseStatus::Free);
    }

    #[test]
    fn no_dot_is_malformed() {
        assert_eq!(check("notadottoken", 0), LicenseStatus::Malformed);
    }

    #[test]
    fn bad_base64_is_malformed() {
        assert_eq!(check("!!!.!!!!", 0), LicenseStatus::Malformed);
    }

    #[test]
    fn valid_base64_bad_sig_is_invalid_signature() {
        // Valid base64url payload + sig bytes, but signature doesn't match any trusted key.
        // kid is a valid 16-char hex fingerprint that won't match any embedded key.
        let payload = URL_SAFE_NO_PAD.encode(b"{\"tier\":\"community\",\"v\":1,\"exp\":9999999999,\"kid\":\"deadbeefdeadbeef\"}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);
        let token   = format!("{payload}.{sig}");
        assert_eq!(check(&token, 0), LicenseStatus::InvalidSignature);
    }

    #[test]
    fn invalid_kid_hex_is_malformed() {
        let payload = URL_SAFE_NO_PAD.encode(b"{\"tier\":\"community\",\"v\":1,\"exp\":9999999999,\"kid\":\"nothex!\"}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);
        let token   = format!("{payload}.{sig}");
        assert_eq!(check(&token, 0), LicenseStatus::Malformed);
    }

    #[test]
    fn free_and_expired_have_no_warning() {
        assert!(LicenseStatus::Free.warning().is_none());
        assert!(LicenseStatus::Expired.warning().is_none());
    }

    #[test]
    fn invalid_malformed_and_version_mismatch_have_warnings() {
        assert!(LicenseStatus::InvalidSignature.warning().is_some());
        assert!(LicenseStatus::Malformed.warning().is_some());
        assert!(LicenseStatus::VersionMismatch.warning().is_some());
    }
}
