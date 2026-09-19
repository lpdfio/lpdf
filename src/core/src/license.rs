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

// The product this engine is.  Every Codesense product signs with its own key, so a foreign key
// is normally refused as `UnknownKey` long before this matters — but that is key hygiene, and a
// rule the engine states itself costs nothing and does not depend on it.
const PRODUCT: &str = "lpdf";

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
    /// Token verified, but was issued for another Codesense product.
    WrongProduct,
    /// Token names a signing key this build does not trust — a key from another environment,
    /// or one newer than this engine.  Distinct from [`LicenseStatus::BadSignature`] because
    /// the two point at completely different causes.
    UnknownKey,
    /// Token names a trusted signing key, but the signature over the payload does not check
    /// out: tampering, or a signer that signed the wrong bytes.
    BadSignature,
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
            LicenseStatus::WrongProduct => {
                Some("license key was issued for a different product — running in free mode")
            }
            LicenseStatus::UnknownKey => {
                Some("license token was signed by a key this build does not trust — running in free mode")
            }
            LicenseStatus::BadSignature => {
                Some("license token has an invalid signature — running in free mode")
            }
            LicenseStatus::Malformed => {
                Some("license token is malformed — running in free mode")
            }
            _ => None,
        }
    }

    /// The stable name this status is reported under outside the engine — the `status` field of
    /// [`report_json`], the CLI's output, and every SDK's result.  Distinct from [`warning`],
    /// which is prose for the end of a render; these are values callers branch on.
    ///
    /// [`warning`]: LicenseStatus::warning
    pub fn code(&self) -> &'static str {
        match self {
            LicenseStatus::Licensed(_)     => "licensed",
            LicenseStatus::Free            => "free",
            LicenseStatus::Expired         => "expired",
            LicenseStatus::VersionMismatch => "version_mismatch",
            LicenseStatus::WrongProduct    => "wrong_product",
            LicenseStatus::UnknownKey      => "unknown_key",
            LicenseStatus::BadSignature    => "bad_signature",
            LicenseStatus::Malformed       => "malformed",
        }
    }
}

/// The claims an engine reads from a token.
///
/// Only ever built from a token whose signature verified: an unverified payload can say anything
/// at all, so reporting its claims would be repeating a stranger's assertions as fact.
#[derive(Debug, PartialEq)]
pub struct LicenseClaims {
    /// Which Codesense product the key was issued for.
    pub product: String,
    /// `community`, `professional` or `enterprise`.
    pub tier: String,
    /// Expiry, Unix seconds.  `None` on a version-locked enterprise key.
    pub exp: Option<i64>,
    /// The license number, as the customer reads it: `L-7K3M9Q`.  `None` on keys minted before
    /// the portal carried it.
    pub license: Option<String>,
    /// Which key this is on its license — 1, 2, 3 in issue order.
    pub key: Option<i64>,
}

impl LicenseClaims {
    /// `exp` as a date a person can read: `2027-09-19T00:00:00Z`.
    ///
    /// `None` on a version-locked enterprise key, which carries no date at all — that is a
    /// different thing from a key whose date happens to be in the past.
    pub fn expires_iso(&self) -> Option<String> {
        self.exp.map(iso8601_utc)
    }
}

/// A token's standing, and what it says about itself once that standing is known.
#[derive(Debug, PartialEq)]
pub struct LicenseReport {
    pub status: LicenseStatus,
    /// `None` whenever the signature did not verify — including for an empty token.
    pub claims: Option<LicenseClaims>,
}

impl LicenseReport {
    fn new(status: LicenseStatus, claims: LicenseClaims) -> Self {
        Self { status, claims: Some(claims) }
    }
}

impl From<LicenseStatus> for LicenseReport {
    /// A verdict reached before anything in the token could be trusted.
    fn from(status: LicenseStatus) -> Self {
        Self { status, claims: None }
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
    report(token, now_unix).status
}

/// [`check`], plus the claims the token carries.
///
/// The render path only needs the status, so it calls `check`.  This is for the `check_license`
/// API, where the caller is a person asking what a key is rather than a renderer deciding
/// whether to draw the attribution line.
pub fn report(token: &str, now_unix: i64) -> LicenseReport {
    if token.is_empty() {
        return LicenseReport::from(LicenseStatus::Free);
    }

    // ── Split <payload>.<signature> ──────────────────────────────────────────
    let dot = match token.find('.') {
        Some(i) => i,
        None    => return LicenseReport::from(LicenseStatus::Malformed),
    };
    let payload_b64 = &token[..dot];
    let sig_b64     = &token[dot + 1..];

    if payload_b64.is_empty() || sig_b64.is_empty() {
        return LicenseReport::from(LicenseStatus::Malformed);
    }

    // ── Decode Base64url ─────────────────────────────────────────────────────
    let payload_bytes = match URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(b)  => b,
        Err(_) => return LicenseReport::from(LicenseStatus::Malformed),
    };
    let sig_bytes = match URL_SAFE_NO_PAD.decode(sig_b64) {
        Ok(b)  => b,
        Err(_) => return LicenseReport::from(LicenseStatus::Malformed),
    };

    // ── Parse JSON claims (untrusted — used only for kid-based key selection) ─
    let claims: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v)  => v,
        Err(_) => return LicenseReport::from(LicenseStatus::Malformed),
    };

    // ── Verify Ed25519 signature ─────────────────────────────────────────────
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s)  => s,
        Err(_) => return LicenseReport::from(LicenseStatus::Malformed),
    };

    // Before the trusted-key check: a token with a bad `kid` is malformed whichever
    // keys this build embeds, including none (a build without LPDF_PUBLIC_KEY).
    let kid = match claims["kid"].as_str() {
        Some(s) => match parse_kid_hex(s) {
            Some(b) => b,
            None    => return LicenseReport::from(LicenseStatus::Malformed),
        },
        None => return LicenseReport::from(LicenseStatus::Malformed),
    };

    // A build with no keys compiled in trusts nothing, which is the same answer as a `kid` that
    // matches none of them: this engine cannot vouch for the key that signed this token.
    let trusted_key = TRUSTED_KEYS_WITH_KID.iter().find(|(fp, _)| *fp == kid);
    let Some((_, key_bytes)) = trusted_key else {
        return LicenseReport::from(LicenseStatus::UnknownKey);
    };

    let verified = VerifyingKey::from_bytes(key_bytes)
        .map(|vk| vk.verify(&payload_bytes, &signature).is_ok())
        .unwrap_or(false);

    // Split from UnknownKey above deliberately: a trusted key whose signature does not check out
    // means the payload was altered, or whatever signed it signed different bytes — a wholly
    // different investigation from "this key is not one of ours".
    if !verified {
        return LicenseReport::from(LicenseStatus::BadSignature);
    }

    // ── Parse trusted claims (signature verified) ────────────────────────────
    let ver = match claims["v"].as_u64() {
        Some(n) => n as u32,
        None    => return LicenseReport::from(LicenseStatus::Malformed),
    };
    let tier = match claims["tier"].as_str() {
        Some(s) => s.to_string(),
        None    => return LicenseReport::from(LicenseStatus::Malformed),
    };
    let product = match claims["product"].as_str() {
        Some(s) => s.to_string(),
        None    => return LicenseReport::from(LicenseStatus::Malformed),
    };

    // Everything below describes a token this engine has verified, so the claims travel with the
    // status: an expired or wrong-version key still says which license and key it is.
    let details = LicenseClaims {
        product,
        tier,
        exp:     claims["exp"].as_i64(),
        license: claims["lno"].as_str().map(str::to_string),
        key:     claims["kno"].as_i64(),
    };

    // ── Product check — before the version, which would otherwise explain another
    //    product's key as an lpdf key from the wrong era ──────────────────────
    if details.product != PRODUCT {
        return LicenseReport::new(LicenseStatus::WrongProduct, details);
    }

    // ── Version check — all tiers ────────────────────────────────────────────
    if ver != LPDF_MAJOR_VERSION {
        return LicenseReport::new(LicenseStatus::VersionMismatch, details);
    }

    // ── Expiry check — presence-gated, every tier ────────────────────────────
    if let Some(status) = expiry_status(&details.tier, details.exp, now_unix) {
        return LicenseReport::new(status, details);
    }

    let tier = details.tier.clone();
    LicenseReport::new(LicenseStatus::Licensed(tier), details)
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

/// What a key is, as JSON — the answer behind `check_license` in every SDK and `lpdf license`
/// on the command line.
///
/// ```json
/// { "status": "licensed", "product": "lpdf", "tier": "professional",
///   "expires": "2027-09-19T00:00:00Z", "license": "L-7K3M9Q", "key": 3 }
/// ```
///
/// `status` is always present ([`LicenseStatus::code`]).  The rest appears only once the
/// signature verified, so `expired` and `version_mismatch` still name their license while
/// `unknown_key`, `bad_signature` and `malformed` say nothing further — there is nothing
/// trustworthy in them to say.  Fields the token does not carry are omitted rather than
/// reported as null.
///
/// Deliberately absent: the `kid`, and the keys this build trusts.  Neither is secret — the
/// trusted public keys ship inside every binary, and `kid` is a claim anyone can read by
/// decoding the token — but the answer stays as narrow as the question.
pub fn report_json(token: &str, now_unix: i64) -> String {
    let report = report(token, now_unix);

    // Assembled in reading order — the verdict, then what the key is — rather than through a
    // serde_json map, which sorts its keys and would put `expires` before `status`. Order means
    // nothing to a parser and a great deal to someone reading `lpdf license --json` in a
    // terminal. Every value still goes through serde_json, so escaping is not hand-rolled.
    let mut fields: Vec<(&str, serde_json::Value)> =
        vec![("status", report.status.code().into())];

    if let Some(claims) = report.claims {
        let expires = claims.expires_iso();

        fields.push(("product", claims.product.into()));
        fields.push(("tier", claims.tier.into()));
        // Absent claims are left out rather than reported as null: a version-locked key has no
        // date, and a key minted before license numbers existed carries no license at all.
        if let Some(expires) = expires {
            fields.push(("expires", expires.into()));
        }
        if let Some(license) = claims.license {
            fields.push(("license", license.into()));
        }
        if let Some(key) = claims.key {
            fields.push(("key", key.into()));
        }
    }

    let body = fields
        .into_iter()
        .map(|(name, value)| format!("\"{name}\":{value}"))
        .collect::<Vec<_>>()
        .join(",");

    format!("{{{body}}}")
}

/// `exp` as a date a person can read: `2027-09-19T00:00:00Z`.
///
/// A Unix integer is the right thing to sign and the wrong thing to paste into a support
/// thread.  Written out here rather than pulled from a date crate: the engine compiles to wasm
/// and carries no time handling at all beyond this.
fn iso8601_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60,
    )
}

/// Days since the epoch to a calendar date, by Howard Hinnant's `civil_from_days`.
///
/// The algorithm shifts the year to start in March so that the leap day lands at the end of it,
/// which is what removes every special case from the month arithmetic.  Correct for any date
/// the proleptic Gregorian calendar covers.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Re-base onto 0000-03-01, the start of the first 400-year era.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);                                     // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);       // [0, 365]
    let month_shifted = (5 * day_of_year + 2) / 153;                                  // [0, 11], March = 0

    let day   = (day_of_year - (153 * month_shifted + 2) / 5 + 1) as u32;             // [1, 31]
    let month = if month_shifted < 10 { month_shifted + 3 } else { month_shifted - 9 } as u32;
    let year  = year_of_era + era * 400 + i64::from(month <= 2);

    (year, month, day)
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
    fn a_kid_this_build_does_not_trust_is_an_unknown_key() {
        // Valid base64url payload and signature bytes, with a well-formed kid that matches no
        // embedded key — a key from another environment, or one newer than this engine.
        let payload = URL_SAFE_NO_PAD.encode(b"{\"tier\":\"community\",\"v\":1,\"exp\":9999999999,\"kid\":\"deadbeefdeadbeef\"}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);
        let token   = format!("{payload}.{sig}");
        assert_eq!(check(&token, 0), LicenseStatus::UnknownKey);
    }

    #[test]
    fn a_token_that_failed_verification_reports_no_claims() {
        // Its payload is whatever its author wrote. Repeating a tier or a license number from a
        // token that did not verify would dress an assertion up as a finding.
        let payload = URL_SAFE_NO_PAD.encode(
            b"{\"product\":\"lpdf\",\"tier\":\"enterprise\",\"v\":1,\"kid\":\"deadbeefdeadbeef\",\"lno\":\"L-FAKE01\"}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);

        let report = report(&format!("{payload}.{sig}"), 0);

        assert_eq!(report.status, LicenseStatus::UnknownKey);
        assert_eq!(report.claims, None);
    }

    #[test]
    fn report_json_of_an_unverifiable_token_is_the_status_alone() {
        let payload = URL_SAFE_NO_PAD.encode(b"{\"tier\":\"community\",\"v\":1,\"kid\":\"deadbeefdeadbeef\"}");
        let sig     = URL_SAFE_NO_PAD.encode(&[0u8; 64]);

        assert_eq!(report_json(&format!("{payload}.{sig}"), 0), r#"{"status":"unknown_key"}"#);
    }

    #[test]
    fn report_json_of_no_token_is_free() {
        assert_eq!(report_json("", 0), r#"{"status":"free"}"#);
    }

    /// The token the portal's signer must produce, byte for byte, from a fixed seed and fixed
    /// claims (`LicenseTokenTests.Sign_reproduces_the_golden_token_the_engine_verifies`).
    ///
    /// The pairing is the point. Each repo's tests only ever exercised its own convention, so
    /// when the portal signed the Base64 *text* of the payload and this engine verified the
    /// decoded *bytes*, both suites passed and every key issued was unverifiable. One side
    /// changing the envelope now fails the other side's build.
    ///
    /// The keys embedded in a build come from `LPDF_PUBLIC_KEY`, so this verifies against the
    /// token's own key — RFC 8032 test vector 1 — rather than through [`report`].
    #[test]
    fn the_portals_golden_token_verifies_under_this_engines_convention() {
        const TOKEN: &str = "eyJwcm9kdWN0IjoibHBkZiIsImZtdCI6MSwidGllciI6InByb2Zlc3Npb25hbCIsInYiOjEsImV4cCI6MTc5MDgxMjgwMCwia2lkIjoiMjFmZTMxZGZhMTU0YTI2MSIsImxubyI6IkwtN0szTTlRIiwia25vIjozfQ.Aeh_m5DHcqFLRv6B8RYSApzIQVMLMjhPJu0SDs5TLOzREF4sgXY4McZDYJumqhMMa7G0Ieak_HPWaFFRD0ZfDg";
        const PUBLIC_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

        let (payload_b64, sig_b64) = TOKEN.split_once('.').expect("golden token has a dot");
        let payload = URL_SAFE_NO_PAD.decode(payload_b64).expect("golden payload decodes");
        let signature = Signature::from_slice(
            &URL_SAFE_NO_PAD.decode(sig_b64).expect("golden signature decodes"))
            .expect("golden signature is 64 bytes");

        let key_bytes: [u8; 32] = decode_hex(PUBLIC_KEY).try_into().expect("32-byte public key");
        let key = VerifyingKey::from_bytes(&key_bytes).expect("public key is on the curve");

        key.verify(&payload, &signature).expect("the engine verifies the portal's golden token");

        // And it carries what the portal says it carries, so a claim rename cannot slip through
        // while the signature still checks out.
        let claims: serde_json::Value = serde_json::from_slice(&payload).expect("payload is JSON");
        assert_eq!(claims["product"], "lpdf");
        assert_eq!(claims["lno"], "L-7K3M9Q");
        assert_eq!(claims["kno"], 3);
    }

    fn decode_hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex digits"))
            .collect()
    }

    // ── Dates ────────────────────────────────────────────────────────────────
    // `expires` is the one field a person reads back, so the conversion is pinned rather than
    // trusted: a leap day, a century that is not a leap year, and the epoch itself.

    #[test]
    fn an_expiry_is_reported_as_an_utc_timestamp() {
        assert_eq!(iso8601_utc(1_790_812_800), "2026-10-01T00:00:00Z");
    }

    #[test]
    fn dates_survive_leap_years_and_the_epoch() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_709_164_800), "2024-02-29T00:00:00Z"); // A leap day.
        assert_eq!(iso8601_utc(4_107_542_400), "2100-03-01T00:00:00Z"); // 2100 is not a leap year.
        assert_eq!(iso8601_utc(1_790_812_799), "2026-09-30T23:59:59Z");
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
    fn every_bad_token_state_has_a_warning() {
        assert!(LicenseStatus::UnknownKey.warning().is_some());
        assert!(LicenseStatus::BadSignature.warning().is_some());
        assert!(LicenseStatus::WrongProduct.warning().is_some());
        assert!(LicenseStatus::Malformed.warning().is_some());
        assert!(LicenseStatus::VersionMismatch.warning().is_some());
    }

    #[test]
    fn an_unknown_key_and_a_bad_signature_are_told_apart() {
        // One status until this split, and the two point at unrelated causes: a key from the
        // wrong environment, against a payload that was altered or signed over the wrong bytes.
        assert_ne!(LicenseStatus::UnknownKey.code(), LicenseStatus::BadSignature.code());
        assert_ne!(LicenseStatus::UnknownKey.warning(), LicenseStatus::BadSignature.warning());
    }
}
