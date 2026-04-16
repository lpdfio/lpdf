//! License token verification.
//!
//! Token format: `<payload_base64url>.<signature_base64url>`
//! - Payload  : UTF-8 JSON, Base64url-encoded (no padding).
//! - Signature: Ed25519 signature over the raw payload bytes, Base64url-encoded.
//!
//! The 32-byte public key is embedded as `PUBLIC_KEY_BYTES` below.
//!
//! # Local test setup
//! Run `npm start` inside `src/license/` once to auto-generate a keypair.
//! The server prints a Rust constant — paste it over `PUBLIC_KEY_BYTES` and
//! rebuild.  The server's `keys/private.hex` is gitignored; never commit it.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey, Verifier};

// ---------------------------------------------------------------------------
// Embedded public key
// ---------------------------------------------------------------------------

/// 32-byte Ed25519 public key.
///
/// **Replace with the bytes printed by `npm start` in `src/license/` before
/// building a real binary.**  The all-zero placeholder causes every token to
/// fail verification and fall back to free mode.
pub const PUBLIC_KEY_BYTES: [u8; 32] = [0u8; 32];

// ---------------------------------------------------------------------------
// Status type
// ---------------------------------------------------------------------------

/// Result of checking a license token.
#[derive(Debug, PartialEq)]
pub enum LicenseStatus {
    /// Token is valid and not expired.  Inner value is the plan name.
    Licensed(String),
    /// No token was supplied — expected free-mode usage, no warning needed.
    Free,
    /// Token carried a valid signature but has passed its `exp` timestamp.
    Expired,
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

    // ── Verify Ed25519 signature ─────────────────────────────────────────────
    let verifying_key = match VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES) {
        Ok(k)  => k,
        Err(_) => {
            // The embedded key constant is invalid (e.g. all-zero placeholder).
            // Treat every token as having a bad signature.
            return LicenseStatus::InvalidSignature;
        }
    };
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s)  => s,
        Err(_) => return LicenseStatus::Malformed,
    };
    if verifying_key.verify(&payload_bytes, &signature).is_err() {
        return LicenseStatus::InvalidSignature;
    }

    // ── Parse JSON claims ────────────────────────────────────────────────────
    let claims: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v)  => v,
        Err(_) => return LicenseStatus::Malformed,
    };

    let exp = match claims["exp"].as_i64() {
        Some(n) => n,
        None    => return LicenseStatus::Malformed,
    };
    let plan = match claims["plan"].as_str() {
        Some(s) => s.to_string(),
        None    => return LicenseStatus::Malformed,
    };

    // ── Expiry check ─────────────────────────────────────────────────────────
    if now_unix > 0 && now_unix > exp {
        return LicenseStatus::Expired;
    }

    LicenseStatus::Licensed(plan)
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
        // Valid base64url payload, valid base64url sig — but key is all-zero
        // placeholder so from_bytes fails → InvalidSignature.
        let payload = URL_SAFE_NO_PAD.encode(b"{\"plan\":\"starter\",\"exp\":9999999999}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);
        let token   = format!("{payload}.{sig}");
        assert_eq!(check(&token, 0), LicenseStatus::InvalidSignature);
    }

    #[test]
    fn free_and_expired_have_no_warning() {
        assert!(LicenseStatus::Free.warning().is_none());
        assert!(LicenseStatus::Expired.warning().is_none());
    }

    #[test]
    fn invalid_and_malformed_have_warnings() {
        assert!(LicenseStatus::InvalidSignature.warning().is_some());
        assert!(LicenseStatus::Malformed.warning().is_some());
    }
}
