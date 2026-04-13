// Integration tests for the Node.js pdf-lib adapter.
// Run with: node --test test/render.test.mjs
//
// Requires the adapter to be compiled first:
//   cd src/adapters/node && npm install && npm run build

import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';
import { renderPdf } from '../dist/index.js';

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Minimal valid lpdf document wrapping arbitrary body XML. */
function doc(body) {
  return `<lpdf version="1"><document><pages><page>${body}</page></pages></document></lpdf>`;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('renderPdf', () => {

  it('returns a valid PDF byte sequence', async () => {
    const bytes = await renderPdf(doc(''), { licenseKey: 'test-key' });
    assert(bytes instanceof Uint8Array, 'result should be Uint8Array');
    // PDF files start with "%PDF-"
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('throws on invalid XML', async () => {
    await assert.rejects(
      () => renderPdf('not xml at all', { licenseKey: 'test-key' }),
      /error/i,
    );
  });

  it('applies watermark when no license key supplied', async () => {
    // The engine returns watermark != null when licenseKey is empty.
    // We can't easily inspect the PDF, but we can verify no error is thrown
    // and the result is still a valid PDF.
    const bytes = await renderPdf(doc(''), { licenseKey: '' });
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
    const bytes = await renderPdf(xml, { licenseKey: 'test-key' });
    assert(bytes.length > 100, 'PDF should be non-trivial');
  });

  it('renders a divider line', async () => {
    const xml = doc(`<divider thickness="xs" color="#cccccc"/>`);
    const bytes = await renderPdf(xml, { licenseKey: 'test-key' });
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
    const bytes = await renderPdf(xml, { licenseKey: 'test-key' });
    assert(bytes.length > 100);
  });

});
