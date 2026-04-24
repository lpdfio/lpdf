// Shared snapshot-test helpers for the Node adapter.
// Centralises paths, SHA-256 hashing, and compare-or-update logic so that
// render.test.mjs only contains test setup and engine invocations.

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const ROOT      = path.resolve(__dirname, '../../../../');
export const FIXTURES  = path.join(ROOT, 'test/fixtures');
export const SNAPSHOTS = path.join(ROOT, 'test/snapshots');
export const UPDATE    = !!process.env.UPDATE_SNAPSHOTS;

export const EXAMPLES = [
  'example1', 'example2', 'example3', 'example4',  'example5',
  'example6', 'example7', 'example8', 'example9', 'example10', 'example11',
  'showcase-cluster', 'showcase-flank', 'showcase-frame',
  'showcase-grid', 'showcase-split', 'showcase-stack',
  'showcase-table', 'showcase-barcode', 'showcase-encryption', 'showcase-forms',
  'bench_xs', 'bench_s', 'bench_m', 'bench_l', 'bench_xl',
  'showcase-canvas-layer', 'showcase-canvas-overlay', 'showcase-canvas-rect', 'showcase-canvas-text',
];

/** Returns the contents of a fixture XML file by name (without extension). */
export function readFixture(name) {
  return readFileSync(path.join(FIXTURES, `${name}.xml`), 'utf8');
}

/** Returns the lowercase hex SHA-256 digest of the given byte buffer. */
export function sha256hex(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/**
 * Compares the SHA-256 hash of `bytes` against the stored snapshot for `name`,
 * or writes a new snapshot when UPDATE_SNAPSHOTS=1.
 *
 * Throws an Error if the hash does not match the stored value.
 */
export function compareOrUpdate(name, bytes) {
  const hash = sha256hex(bytes);
  const snap = path.join(SNAPSHOTS, `${name}.pdf.sha256`);

  if (UPDATE) {
    writeFileSync(snap, hash, 'utf8');
    console.log(`  updated snapshot ${name}: ${hash}`);
  } else {
    const stored = readFileSync(snap, 'utf8').trim();
    if (hash !== stored) {
      throw new Error(
        `PDF output changed for ${name}. Run with UPDATE_SNAPSHOTS=1 to accept.\n` +
        `  expected: ${stored}\n  received: ${hash}`,
      );
    }
  }
}
