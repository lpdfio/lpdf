# lpdf license server

A local Node.js tool for generating Ed25519-signed license tokens during development and testing. Not intended for production use.

## Setup

```sh
cd src/license
npm install
npm start
```

The server starts at **http://localhost:4000**.

## First run

On first run the server auto-generates an Ed25519 keypair and saves it to `keys/` (gitignored). It then prints a Rust constant like:

```
─────────────────────────────────────────────────────────
Paste this into src/core/src/license.rs then rebuild:

pub const PUBLIC_KEY_BYTES: [u8; 32] = [
    0x1a, 0x2b, 0x3c, ...
];
─────────────────────────────────────────────────────────
```

Paste that constant over the placeholder in `src/core/src/license.rs`, then rebuild the WASM and WASI binaries:

```sh
make build-wasm
make build-wasi
```

The public key is also available at any time via:

```sh
curl http://localhost:4000/pubkey
```

## Generating a token

### Web UI

Open **http://localhost:4000** and fill in the form:

| Field | Description |
|---|---|
| Email | Customer email address |
| Plan | `starter`, `business`, or `scale` |
| Expiry (days) | Days until the token expires (default 365, max 3650) |
| Customer ID | Optional free-text identifier (e.g. company name or account ID) |

The generated token is shown on screen and appended to `licenses.csv`.

### API

```sh
curl -X POST http://localhost:4000/generate \
  -H "Content-Type: application/json" \
  -d '{"email": "alice@example.com", "plan": "business", "days": 365, "cid": "acme"}'
```

Response:

```json
{
  "id": 1,
  "token": "<base64url-payload>.<base64url-signature>",
  "payload": {
    "cid": "acme",
    "email": "alice@example.com",
    "plan": "business",
    "iat": 1744000000,
    "exp": 1775536000
  }
}
```

### List issued licenses

```sh
curl http://localhost:4000/licenses
```

Returns the raw `licenses.csv` contents.

## Token format

```
<base64url(json_payload)>.<base64url(ed25519_signature)>
```

The payload is UTF-8 JSON with the following fields:

| Field | Type | Description |
|---|---|---|
| `cid` | string | Customer identifier |
| `email` | string | Customer email |
| `plan` | string | `starter`, `business`, or `scale` |
| `iat` | integer | Issued-at Unix timestamp |
| `exp` | integer | Expiry Unix timestamp |

## Validation behaviour

The core engine (`src/core/src/license.rs`) verifies tokens offline using the embedded public key:

| Token state | Engine behaviour |
|---|---|
| Empty string | Free mode — no warning |
| Valid, not expired | Licensed mode — no watermark, producer `lpdf.io` |
| Valid, expired | Free mode — `license_warning` in JSON output |
| Invalid signature | Free mode — `license_warning` in JSON output |
| Malformed / bad Base64 | Free mode — `license_warning` in JSON output |

## Files

```
src/license/
  server.js         Express server + token generation logic
  package.json      Dependencies (@noble/ed25519, @noble/hashes, express)
  public/
    index.html      Web UI
  keys/             Auto-generated keypair (gitignored)
    private.hex
    public.hex
  licenses.csv      Issued tokens log (gitignored)
```

## Security notes

- `keys/private.hex` is gitignored. Back it up securely — tokens can only be verified against the matching public key baked into the binary.
- Regenerating the keypair invalidates all previously issued tokens. Rebuild and redeploy the WASM/WASI binaries after any key rotation.
- This server has no authentication. Run it on localhost only.

## What to commit

| File | Commit? | Reason |
|---|---|---|
| `PUBLIC_KEY_BYTES` in `license.rs` | ✅ Yes | Public key — safe by design, required in the binary |
| `keys/public.hex` | Optional | Redundant — the value is already in `license.rs` |
| `keys/private.hex` | ❌ Never | Signs tokens — if leaked, anyone can forge licenses |
| `licenses.csv` | ❌ Never | Customer data |

### Dev vs production keypair

- **Development / testing** — commit `license.rs` with the current key. Before a production release, generate a fresh keypair on a secure machine, paste the new `PUBLIC_KEY_BYTES`, and rebuild. Tokens signed with the dev key will stop working after the rotation (by design).
- **Production** — generate the keypair once on a secure machine, commit only `PUBLIC_KEY_BYTES` in `license.rs`, and store `keys/private.hex` in a secrets vault (password manager, HSM, etc.).
