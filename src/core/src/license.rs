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
//! | `exp`  | integer | community / pro | Unix timestamp; optional on enterprise tokens     |
//! | `kid`  | string  | all tiers       | 16-char hex fingerprint of the signing key       |
//!
//! ## Validation logic
//!
//! 1. Verify Ed25519 signature (using `kid` for direct key lookup if present).
//! 2. Check `v == LPDF_MAJOR_VERSION` — all tiers; mismatch → [`LicenseStatus::VersionMismatch`].
//! 3. Check `exp > now_unix` wherever `exp` is present → [`LicenseStatus::Expired`].
//!
//! Expiry is gated on the **presence of the claim**, not on the tier. The portal
//! decides a key's mode when it mints it, and `exp` records that decision; both
//! sides then read the same signal instead of each hard-coding a list of tier
//! names that has to be kept in step.
//!
//! An enterprise key may be either: dated, or version-locked with no `exp` at
//! all, where the ongoing contract is the enforcement mechanism. Community and
//! professional keys must always carry one. All keys, dated or not, become
//! invalid on major version change — the customer generates a new key from the
//! portal as part of their upgrade.
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

    // Before the trusted-key check: a token with a bad `kid` is malformed whichever
    // keys this build embeds, including none (a build without LPDF_PUBLIC_KEY).
    let kid = match claims["kid"].as_str() {
        Some(s) => match parse_kid_hex(s) {
            Some(b) => b,
            None    => return LicenseStatus::Malformed,
        },
        None => return LicenseStatus::Malformed,
    };

    if TRUSTED_KEYS_WITH_KID.is_empty() {
        return LicenseStatus::InvalidSignature;
    }

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

    // ── Expiry check — presence-gated, every tier ────────────────────────────
    if let Some(status) = expiry_status(&tier, claims["exp"].as_i64(), now_unix) {
        return status;
    }

    LicenseStatus::Licensed(tier)
}

/// A token's date standing, from its tier and its `exp` claim.
///
/// `exp` is enforced wherever it appears, whatever the tier: the claim itself is the
/// discriminator, so the engine and the portal that mints the key agree by construction
/// rather than by both hard-coding the same list of tier names. Gating on the tier instead
/// meant an enterprise key could carry a date that nothing checked.
///
/// Only enterprise may omit `exp` — those keys are version-locked and the contract governs
/// date-based use. A community or professional token without one stays [`Malformed`] rather
/// than becoming perpetual, which is what keeps a portal bug from minting a key that never
/// lapses. Claims are signed, so this is an integrity check, not a defence against tampering.
///
/// `now_unix` of `0` means "no clock" — WASM and WASI have no system time — and skips the
/// comparison rather than treating every key as expired.
///
/// Split out from [`check`] so the rules can be tested without a signed token.
fn expiry_status(tier: &str, exp: Option<i64>, now_unix: i64) -> Option<LicenseStatus> {
    match exp {
        Some(exp) if now_unix > 0 && now_unix > exp => Some(LicenseStatus::Expired),
        Some(_) => None,
        None if tier.eq_ignore_ascii_case("enterprise") => None,
        None => Some(LicenseStatus::Malformed),
    }
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

    // ── Expiry rules ─────────────────────────────────────────────────────────
    // The surrounding verification needs a real signature to exercise, so these go through
    // `expiry_status` directly. `NOW` is an arbitrary clock reading, `EXP` an hour earlier.
    const NOW: i64 = 1_800_000_000;
    const EXP: i64 = NOW - 3_600;

    #[test]
    fn a_dated_enterprise_key_expires_like_any_other() {
        // The whole point of the change: this used to return Licensed for ever.
        assert_eq!(expiry_status("enterprise", Some(EXP), NOW), Some(LicenseStatus::Expired));
    }

    #[test]
    fn an_enterprise_key_with_no_date_is_version_locked_not_expired() {
        assert_eq!(expiry_status("enterprise", None, NOW), None);
    }

    #[test]
    fn a_community_or_pro_key_with_no_date_stays_malformed() {
        // Not perpetual: a portal bug that dropped `exp` must not mint a key that never lapses.
        assert_eq!(expiry_status("community", None, NOW), Some(LicenseStatus::Malformed));
        assert_eq!(expiry_status("professional", None, NOW), Some(LicenseStatus::Malformed));
    }

    #[test]
    fn a_key_still_inside_its_term_passes() {
        assert_eq!(expiry_status("professional", Some(NOW + 3_600), NOW), None);
    }

    #[test]
    fn expiry_is_skipped_when_the_host_has_no_clock() {
        // WASM and WASI pass 0; a past date must not fail there.
        assert_eq!(expiry_status("community", Some(EXP), 0), None);
    }

    #[test]
    fn the_tier_name_is_matched_case_insensitively() {
        assert_eq!(expiry_status("Enterprise", None, NOW), None);
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
