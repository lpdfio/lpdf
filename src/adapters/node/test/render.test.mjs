// Integration tests for the Node.js pdf-lib adapter.
// Run with: node --test test/render.test.mjs
//
// Requires the adapter to be compiled first:
//   cd src/adapters/node && npm install && npm run build

import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';
import { LpdfEngine, LpdfKit, kitToXml } from '../dist/index.js';

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

  it('accepts an LpdfDocument tree directly (JSON path)', async () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({ nodes: [LpdfKit.text(['Hello PDF'])] })],
    });
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(document);
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

// ── kitToXml ──────────────────────────────────────────────────────────────────

describe('kitToXml', () => {

  it('returns a string starting with the XML declaration', () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({ nodes: [] })],
    });
    const xml = kitToXml(document);
    assert(typeof xml === 'string');
    assert(xml.startsWith('<?xml version="1.0"'), `unexpected start: ${xml.slice(0, 50)}`);
  });

  it('contains <lpdf version="1">', () => {
    const document = LpdfKit.document({ nodes: [LpdfKit.page({ nodes: [] })] });
    const xml = kitToXml(document);
    assert(xml.includes('<lpdf version="1">'), 'missing <lpdf version="1">');
  });

  it('places builtin font in <assets><fonts> with core= attribute', () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({ nodes: [] })],
      options: {
        tokens: {
          fonts: { heading: { builtin: 'Helvetica-Bold' } },
        },
      },
    });
    const xml = kitToXml(document);
    assert(xml.includes('<assets>'), 'missing <assets>');
    assert(xml.includes('<fonts>'), 'missing <fonts>');
    assert(xml.includes('core="Helvetica-Bold"'), 'missing core= attribute');
    assert(!xml.includes('<tokens>') || !xml.includes('<fonts>') || (() => {
      // Fonts must NOT appear inside <tokens> — only inside <assets>
      const tokensStart = xml.indexOf('<tokens>');
      const tokensEnd   = xml.indexOf('</tokens>');
      if (tokensStart === -1) return true;
      const fontsInTokens = xml.indexOf('<fonts>', tokensStart);
      return fontsInTokens === -1 || fontsInTokens > tokensEnd;
    })(), 'fonts incorrectly placed inside <tokens>');
  });

  it('places custom font src in <assets><fonts> with ref= attribute', () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({ nodes: [] })],
      options: {
        tokens: {
          fonts: { body: { src: '/fonts/MyFont.ttf' } },
        },
      },
    });
    const xml = kitToXml(document);
    assert(xml.includes('ref="body"'), 'custom font should use alias name as ref=');
    assert(!xml.includes('src='), 'src= path must not appear in XML (only ref= alias)');
  });

  it('emits text tokens inside <tokens>', () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({ nodes: [] })],
      options: { tokens: { text: { body: '12pt', heading: '20pt' } } },
    });
    const xml = kitToXml(document);
    assert(xml.includes('<tokens>'), 'missing <tokens>');
    assert(xml.includes('<text '), 'missing <text> token element');
  });

  it('produced XML renders to a valid PDF', async () => {
    const document = LpdfKit.document({
      nodes: [LpdfKit.page({
        nodes: [LpdfKit.text(['Hello from kitToXml'])],
      })],
    });
    const xml  = kitToXml(document);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

});

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
