/**
 * lpdf local license generator — development / testing only.
 *
 * On first run, generates an Ed25519 keypair and saves it to keys/.
 * Prints the Rust PUBLIC_KEY_BYTES constant — paste it into
 * src/core/src/license.rs and rebuild the WASM/WASI binary.
 *
 * Endpoints:
 *   POST /generate  { email, plan, days?, cid? }  → { token, payload }
 *   GET  /licenses                                 → CSV text
 *
 * Generated tokens are appended to licenses.csv.
 */

import * as ed    from "@noble/ed25519";
import { sha512 } from "@noble/hashes/sha512";
import express    from "express";
import fs         from "fs";
import path       from "path";
import { fileURLToPath } from "url";

// noble/ed25519 v2 requires a synchronous SHA-512 implementation.
ed.etc.sha512Sync = (...msgs) => sha512(...msgs);

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------
const KEYS_DIR    = path.join(__dirname, "keys");
const PRIV_FILE   = path.join(KEYS_DIR,  "private.hex");
const PUB_FILE    = path.join(KEYS_DIR,  "public.hex");
const CSV_FILE    = path.join(__dirname, "licenses.csv");
const PUBLIC_DIR  = path.join(__dirname, "public");

const VALID_PLANS = ["community", "professional", "enterprise"];
const CSV_HEADER  = "id,cid,email,plan,iat,exp,token\n";

// ---------------------------------------------------------------------------
// Keypair — auto-generate on first run
// ---------------------------------------------------------------------------
function ensureKeys() {
  if (!fs.existsSync(KEYS_DIR)) fs.mkdirSync(KEYS_DIR, { recursive: true });

  if (!fs.existsSync(PRIV_FILE)) {
    const privKey = ed.utils.randomPrivateKey();
    const pubKey  = ed.getPublicKey(privKey);

    fs.writeFileSync(PRIV_FILE, toHex(privKey), "utf8");
    fs.writeFileSync(PUB_FILE,  toHex(pubKey),  "utf8");
    console.log("\n✔ Generated new Ed25519 keypair → keys/");
  }

  // Always print the Rust constant so it's visible on every start.
  printRustConstant(loadPubKey());
}

function loadPrivKey() {
  return fromHex(fs.readFileSync(PRIV_FILE, "utf8").trim());
}

function loadPubKey() {
  return fromHex(fs.readFileSync(PUB_FILE, "utf8").trim());
}

function printRustConstant(pubKeyBytes) {
  const hexEnv = toHex(pubKeyBytes);
  console.log("\n─────────────────────────────────────────────────────────");
  console.log("Set this environment variable then rebuild:\n");
  console.log(`  export LPDF_PUBLIC_KEY=${hexEnv}\n`);
  console.log("For CI / GitHub Actions, add it as a repository secret");
  console.log("named LPDF_PUBLIC_KEY and add to every cargo step:\n");
  console.log("  env:");
  console.log("    LPDF_PUBLIC_KEY: ${{ secrets.LPDF_PUBLIC_KEY }}");
  console.log("\nFor key rotation, prepend the new key (comma-separated):");
  console.log(`  LPDF_PUBLIC_KEY=<new_key>,${hexEnv}`);
  console.log("─────────────────────────────────────────────────────────\n");
}

// ---------------------------------------------------------------------------
// CSV helpers
// ---------------------------------------------------------------------------
function ensureCsv() {
  if (!fs.existsSync(CSV_FILE)) {
    fs.writeFileSync(CSV_FILE, CSV_HEADER, "utf8");
  }
}

function appendLicense(row) {
  fs.appendFileSync(CSV_FILE, row + "\n", "utf8");
}

function readLicenses() {
  ensureCsv();
  return fs.readFileSync(CSV_FILE, "utf8");
}

function nextId() {
  ensureCsv();
  const lines = fs.readFileSync(CSV_FILE, "utf8").trim().split("\n").filter(Boolean);
  // lines[0] is header; count data rows
  return Math.max(1, lines.length);
}

// ---------------------------------------------------------------------------
// Token generation
// ---------------------------------------------------------------------------
function base64url(bytes) {
  return Buffer.from(bytes)
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

function issueToken(privKey, payload) {
  const payloadBytes = Buffer.from(JSON.stringify(payload), "utf8");
  const payloadB64   = base64url(payloadBytes);
  const sig          = ed.sign(payloadBytes, privKey);
  const sigB64       = base64url(sig);
  return `${payloadB64}.${sigB64}`;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function toHex(bytes) {
  return Buffer.from(bytes).toString("hex");
}
function fromHex(hex) {
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

function sanitize(str, maxLen = 200) {
  if (typeof str !== "string") return "";
  return str.replace(/[\r\n\t,]/g, " ").trim().slice(0, maxLen);
}

// ---------------------------------------------------------------------------
// Express app
// ---------------------------------------------------------------------------
const app = express();
app.use(express.json());
app.use(express.static(PUBLIC_DIR));

// POST /generate
app.post("/generate", (req, res) => {
  const email = sanitize(req.body?.email ?? "");
  const plan  = sanitize(req.body?.plan  ?? "");
  const cid   = sanitize(req.body?.cid   ?? "");
  const days  = Math.max(1, Math.min(3650, parseInt(req.body?.days ?? "365", 10) || 365));

  if (!email) return res.status(400).json({ error: "email is required" });
  if (!VALID_PLANS.includes(plan)) {
    return res.status(400).json({ error: `plan must be one of: ${VALID_PLANS.join(", ")}` });
  }

  const privKey = loadPrivKey();
  const now     = Math.floor(Date.now() / 1000);
  const exp     = now + days * 24 * 3600;

  const payload = {
    cid:   cid || `local-${Date.now()}`,
    email,
    plan,
    iat:   now,
    exp,
  };

  const token = issueToken(privKey, payload);
  const id    = nextId();

  appendLicense(
    `${id},${sanitize(payload.cid)},${email},${plan},${now},${exp},${token}`
  );

  res.json({ token, payload, id });
});

// GET /licenses
app.get("/licenses", (_req, res) => {
  res.type("text/plain").send(readLicenses());
});

// GET /pubkey — returns the public key Rust constant for easy copying
app.get("/pubkey", (_req, res) => {
  const pubKey = loadPubKey();
  const hex    = [...pubKey].map(b => `0x${b.toString(16).padStart(2, "0")}`);
  const rows   = [];
  for (let i = 0; i < hex.length; i += 8) {
    rows.push("    " + hex.slice(i, i + 8).join(", "));
  }
  res.type("text/plain").send(
    `pub const PUBLIC_KEY_BYTES: [u8; 32] = [\n${rows.join(",\n")},\n];`
  );
});

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------
ensureKeys();
ensureCsv();

const PORT = 4000;
app.listen(PORT, () => {
  console.log(`lpdf license server running at http://localhost:${PORT}`);
});
