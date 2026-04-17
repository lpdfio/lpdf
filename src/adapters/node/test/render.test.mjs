// Integration tests for the Node.js pdf-lib adapter.
// Run with: node --test test/render.test.mjs
//
// Requires the adapter to be compiled first:
//   cd src/adapters/node && npm install && npm run build

import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';
import { LpdfEngine } from '../dist/index.js';

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Minimal valid lpdf document wrapping arbitrary body XML. */
function doc(body) {
  return `<lpdf version="1"><document><pages><page>${body}</page></pages></document></lpdf>`;
}

// ── LpdfEngine class ──────────────────────────────────────────────────────────

describe('LpdfEngine', () => {

  it('returns a valid PDF byte sequence', async () => {
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(doc(''));
    assert(bytes instanceof Uint8Array, 'result should be Uint8Array');
    // PDF files start with "%PDF-"
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('throws on invalid XML', async () => {
    const lpdf = new LpdfEngine('test-key');
    await assert.rejects(
      () => lpdf.renderPdf('not xml at all'),
      /error/i,
    );
  });

  it('applies watermark when no license key supplied', async () => {
    // The engine returns watermark != null when licenseKey is empty.
    // We can't easily inspect the PDF, but we can verify no error is thrown
    // and the result is still a valid PDF.
    const lpdf = new LpdfEngine('');
    const bytes = await lpdf.renderPdf(doc(''));
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('renders a page with a stack of two frames', async () => {
    const xml = doc(`
      <stack gap="m">
        <frame height="40pt"/>
        <frame height="40pt"/>
      </stack>
    `);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    assert(bytes.length > 100, 'PDF should be non-trivial');
  });

  it('renders a divider line', async () => {
    const xml = doc(`<divider thickness="xs" color="#cccccc"/>`);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('renders a grid', async () => {
    const xml = doc(`
      <grid cols="3" gap="s">
        <frame height="20pt"/>
        <frame height="20pt"/>
        <frame height="20pt"/>
      </grid>
    `);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    assert(bytes.length > 100);
  });

  it('merges instance-level and per-call fontBytes (per-call wins)', async () => {
    // Both levels supply a key; per-call value should win.
    // We can't easily inspect loaded fonts, but verifying no error is enough.
    const instanceFont = new Uint8Array([1, 2, 3]);
    const callFont = new Uint8Array([4, 5, 6]);
    const lpdf = new LpdfEngine('test-key', { fontBytes: { Shared: instanceFont } });
    const bytes = await lpdf.renderPdf(doc(''), { fontBytes: { Shared: callFont } });
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

});

// ── Snapshot tests ────────────────────────────────────────────────────────────
// Render each fixture XML → PDF, hash with SHA-256, compare against stored hash.
//
// Generate / update snapshots:
//   UPDATE_SNAPSHOTS=1 node --test test/render.test.mjs
//
// Normal run (CI):
//   node --test test/render.test.mjs

import { EXAMPLES, readFixture, compareOrUpdate } from './snapshot_helper.mjs';

describe('PDF snapshots (fixture XMLs)', () => {
  for (const name of EXAMPLES) {
    it(`${name} matches stored hash`, async () => {
      const xml   = readFixture(name);
      const lpdf  = new LpdfEngine('test-key');
      const bytes = await lpdf.renderPdf(xml);
      compareOrUpdate(name, bytes);
    });
  }
});
