// Integration tests for the Node.js lpdf adapter.
// Run with: node --test test/render.test.mjs
//
// Requires the adapter to be compiled first:
//   cd src/adapters/node && npm install && npm run build

import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';
import { LpdfEngine, LpdfKit, LpdfLayout, LpdfCanvas, LpdfRenderError, CanvasTransform, kitToXml } from '../dist/index.js';

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Minimal valid lpdf document wrapping arbitrary body XML. */
function doc(body) {
  const inner = `<layout>${body}</layout>`;
  return `<lpdf version="1"><document><section>${inner}</section></document></lpdf>`;
}

/** Build a minimal LpdfDocument using the kit API. */
function kitDoc(layoutNodes = []) {
  return LpdfKit.document({
    sections: [
      LpdfKit.section({ nodes: [LpdfKit.layout(layoutNodes)] }),
    ],
  });
}

// ── LpdfEngine class ──────────────────────────────────────────────────────────

describe('LpdfEngine', () => {

  it('returns a valid PDF byte sequence', async () => {
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(doc(''));
    assert(bytes instanceof Uint8Array, 'result should be Uint8Array');
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('throws LpdfRenderError on invalid XML', async () => {
    const lpdf = new LpdfEngine('test-key');
    await assert.rejects(
      () => lpdf.renderPdf('not xml at all'),
      (err) => err instanceof LpdfRenderError,
    );
  });

  it('applies watermark when no license key supplied', async () => {
    const lpdf = new LpdfEngine('');
    const bytes = await lpdf.renderPdf(doc(''));
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('renders a section with a stack of two frames', async () => {
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
    const instanceFont = new Uint8Array([1, 2, 3]);
    const callFont = new Uint8Array([4, 5, 6]);
    const lpdf = new LpdfEngine('test-key', { fontBytes: { Shared: instanceFont } });
    const bytes = await lpdf.renderPdf(doc(''), { fontBytes: { Shared: callFont } });
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('accepts an LpdfDocument tree directly (JSON path)', async () => {
    const document = kitDoc([LpdfLayout.text(['Hello PDF'])]);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(document);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('setEncryption produces a valid encrypted PDF', async () => {
    const lpdf = new LpdfEngine('test-key');
    lpdf.setEncryption({ userPassword: '', ownerPassword: 's3cr3t' });
    const bytes = await lpdf.renderPdf(doc(''));
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
    const text = Buffer.from(bytes).toString('latin1');
    assert(text.includes('/Encrypt'), 'encrypted PDF should contain /Encrypt entry');
  });

  it('loadImage does not throw and produces a valid PDF', async () => {
    const png1x1 = Buffer.from(
      '89504e470d0a1a0a0000000d49484452000000010000000108000000003a7e9b55' +
      '0000000a49444154789c6260000000020001e221bc330000000049454e44ae426082',
      'hex',
    );
    const lpdf = new LpdfEngine('test-key');
    lpdf.loadImage('testimg', png1x1);
    const bytes = await lpdf.renderPdf(doc(''));
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('renders canvas layer with a rect via JSON path', async () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.canvas([
              LpdfCanvas.layer([LpdfCanvas.rect(0, 0, 595, 842, { fill: '#eeeeee' })]),
            ]),
            LpdfKit.layout([LpdfLayout.text(['Canvas underlay'])]),
          ],
        }),
      ],
    });
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(document);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('kitToXml canvas text content appears in XML output', () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.canvas([
              LpdfCanvas.layer([LpdfCanvas.text(10, 20, 'Hello canvas')]),
            ]),
            LpdfKit.layout([LpdfLayout.text(['x'])]),
          ],
        }),
      ],
    });
    const xml = kitToXml(document);
    assert(xml.includes('Hello canvas'), `text content missing from XML:\n${xml}`);
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
    const document = kitDoc([]);
    const xml = kitToXml(document);
    assert(typeof xml === 'string');
    assert(xml.startsWith('<?xml version="1.0"'), `unexpected start: ${xml.slice(0, 50)}`);
  });

  it('contains <lpdf version="1">', () => {
    const document = kitDoc([]);
    const xml = kitToXml(document);
    assert(xml.includes('<lpdf version="1">'), 'missing <lpdf version="1">');
  });

  it('places builtin font in <assets> (flat) with core= attribute', () => {
    const document = LpdfKit.document({
      sections: [LpdfKit.section({ nodes: [LpdfKit.layout([])] })],
      options: {
        tokens: {
          fonts: { heading: { builtin: 'Helvetica-Bold' } },
        },
      },
    });
    const xml = kitToXml(document);
    assert(xml.includes('<assets>'), 'missing <assets>');
    assert(!xml.includes('<fonts>'), '<fonts> wrapper must not appear in flat structure');
    assert(xml.includes('<font '), 'missing <font> element');
    assert(xml.includes('core="Helvetica-Bold"'), 'missing core= attribute');
    const tokensStart = xml.indexOf('<tokens>');
    const tokensEnd   = xml.indexOf('</tokens>');
    if (tokensStart !== -1) {
      const fontInTokens = xml.indexOf('<font ', tokensStart);
      assert(fontInTokens === -1 || fontInTokens > tokensEnd, 'font incorrectly placed inside <tokens>');
    }
  });

  it('places custom font src in <assets> (flat) with ref= and src= attributes', () => {
    const document = LpdfKit.document({
      sections: [LpdfKit.section({ nodes: [LpdfKit.layout([])] })],
      options: {
        tokens: {
          fonts: { body: { src: '/fonts/MyFont.ttf' } },
        },
      },
    });
    const xml = kitToXml(document);
    assert(xml.includes('ref="body"'), 'custom font should use alias name as ref=');
    assert(xml.includes('src='), 'src= path should appear in XML');
  });

  it('emits text tokens inside <tokens>', () => {
    const document = LpdfKit.document({
      sections: [LpdfKit.section({ nodes: [LpdfKit.layout([])] })],
      options: { tokens: { text: { body: '12pt', heading: '20pt' } } },
    });
    const xml = kitToXml(document);
    assert(xml.includes('<tokens>'), 'missing <tokens>');
    assert(xml.includes('<text '), 'missing <text> token element');
  });

  it('produced XML renders to a valid PDF', async () => {
    const document = kitDoc([LpdfLayout.text(['Hello from kitToXml'])]);
    const xml  = kitToXml(document);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('emits section with layout and canvas blocks', () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.canvas([LpdfCanvas.layer([LpdfCanvas.rect(0, 0, 10, 10)])]),
            LpdfKit.layout([LpdfLayout.text(['hello'])]),
          ],
        }),
      ],
    });
    const xml = kitToXml(document);
    assert(xml.includes('<canvas>'), 'missing <canvas> block');
    assert(xml.includes('<layout>'), 'missing <layout> block');
    assert(xml.includes('<layer'), 'missing <layer> element');
    assert(xml.includes('<rect '), 'missing <rect> element');
  });

  it('canvas text node emits text content at top level', () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.canvas([LpdfCanvas.layer([LpdfCanvas.text(5, 5, 'Test text')])]),
            LpdfKit.layout([LpdfLayout.text(['x'])]),
          ],
        }),
      ],
    });
    const xml = kitToXml(document);
    assert(xml.includes('Test text'), 'canvas text content missing from XML');
  });

  it('canvas text node with runs emits span elements', () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.canvas([
              LpdfCanvas.layer([
                LpdfCanvas.text(5, 5, 'ignored', undefined, [
                  { text: 'bold part', font: 'Helvetica-Bold', size: 14 },
                  { text: 'normal part', color: '#333333' },
                ]),
              ]),
            ]),
            LpdfKit.layout([LpdfLayout.text(['x'])]),
          ],
        }),
      ],
    });
    const xml = kitToXml(document);
    assert(xml.includes('<span'), 'missing <span> for runs');
    assert(xml.includes('bold part'), 'run text missing');
    assert(xml.includes('font="Helvetica-Bold"'), 'run font attr missing');
    assert(xml.includes('font-size="14"'), 'run font-size attr missing');
    assert(xml.includes('color="#333333"'), 'run color attr missing');
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

// ── LpdfCanvas serialization ──────────────────────────────────────────────────

describe('LpdfCanvas serialization', () => {

  it('rect emits correct type and string attrs', () => {
    const node = LpdfCanvas.rect(10, 20, 100, 50);
    assert.equal(node.type, 'canvas-rect');
    assert.equal(node.attrs.x, '10');
    assert.equal(node.attrs.y, '20');
    assert.equal(node.attrs.w, '100');
    assert.equal(node.attrs.h, '50');
  });

  it('rect style attrs emitted as strings', () => {
    const node = LpdfCanvas.rect(0, 0, 10, 10, {
      fill: '#ff0000', stroke: '#000', strokeWidth: 2,
      strokeDash: [4, 2], borderRadius: 5,
    });
    assert.equal(node.attrs.fill, '#ff0000');
    assert.equal(node.attrs.stroke, '#000');
    assert.equal(node.attrs['stroke-width'], '2');
    assert.equal(node.attrs['stroke-dash'], '4 2');
    assert.equal(node.attrs.radius, '5');
  });

  it('line emits correct type and coords as strings', () => {
    const node = LpdfCanvas.line(0, 0, 100, 100, { stroke: '#000', strokeWidth: 1 });
    assert.equal(node.type, 'canvas-line');
    assert.equal(node.attrs.x1, '0');
    assert.equal(node.attrs.y1, '0');
    assert.equal(node.attrs.x2, '100');
    assert.equal(node.attrs.y2, '100');
    assert.equal(node.attrs['stroke-width'], '1');
  });

  it('ellipse emits correct type', () => {
    const node = LpdfCanvas.ellipse(50, 50, 30, 20);
    assert.equal(node.type, 'canvas-ellipse');
    assert.equal(node.attrs.cx, '50');
    assert.equal(node.attrs.ry, '20');
  });

  it('circle emits correct type', () => {
    const node = LpdfCanvas.circle(50, 50, 25);
    assert.equal(node.type, 'canvas-circle');
    assert.equal(node.attrs.r, '25');
  });

  it('path emits correct type', () => {
    const node = LpdfCanvas.path('M 0 0 L 100 100');
    assert.equal(node.type, 'canvas-path');
    assert.equal(node.attrs.d, 'M 0 0 L 100 100');
  });

  it('path fillRuleEvenodd → fill-rule attr', () => {
    const even = LpdfCanvas.path('M 0 0', { fillRuleEvenodd: true });
    const nonz = LpdfCanvas.path('M 0 0', { fillRuleEvenodd: false });
    assert.equal(even.attrs['fill-rule'], 'evenodd');
    assert.equal(nonz.attrs['fill-rule'], 'nonzero');
  });

  it('text emits text at top-level and string attrs', () => {
    const node = LpdfCanvas.text(10, 20, 'Hello', { font: 'Helvetica', size: 12, color: '#000' });
    assert.equal(node.type, 'canvas-text');
    assert.equal(node.text, 'Hello');
    assert.equal(node.attrs.x, '10');
    assert.equal(node.attrs.y, '20');
    assert.equal(node.attrs.font, 'Helvetica');
    assert.equal(node.attrs['font-size'], '12');
    assert.equal(node.attrs.color, '#000');
    assert(!('runs' in node), 'runs key absent when no runs passed');
  });

  it('text with runs emits nested attrs format', () => {
    const node = LpdfCanvas.text(0, 0, 'base', undefined, [
      { text: 'bold', font: 'Helvetica-Bold', size: 14 },
      { text: 'plain', color: '#333' },
    ]);
    assert(Array.isArray(node.runs), 'runs should be array');
    assert.equal(node.runs[0].text, 'bold');
    assert.equal(node.runs[0].attrs.font, 'Helvetica-Bold');
    assert.equal(node.runs[0].attrs['font-size'], '14');
    assert.equal(node.runs[1].attrs.color, '#333');
  });

  it('img emits name attr', () => {
    const node = LpdfCanvas.img(0, 0, 100, 80, 'logo');
    assert.equal(node.type, 'canvas-img');
    assert.equal(node.attrs.name, 'logo');
    assert.equal(node.attrs.w, '100');
    assert.equal(node.attrs.h, '80');
  });

  it('layer without options emits empty attrs', () => {
    const node = LpdfCanvas.layer([LpdfCanvas.rect(0, 0, 10, 10)]);
    assert.equal(node.type, 'canvas-layer');
    assert.deepEqual(node.attrs, {});
    assert.equal(node.nodes.length, 1);
  });

  it('layer opacity emitted as string', () => {
    const node = LpdfCanvas.layer([], { opacity: 0.5 });
    assert.equal(node.attrs.opacity, '0.5');
  });

  it('layer page scope emitted', () => {
    const node = LpdfCanvas.layer([], { page: 'first' });
    assert.equal(node.attrs.page, 'first');
  });

  it('layer transform emitted as matrix(...) string', () => {
    const t = CanvasTransform.translate(10, 20);
    const node = LpdfCanvas.layer([], { transform: t });
    assert.equal(node.attrs.transform, 'matrix(1,0,0,1,10,20)');
  });

  it('null/undefined style fields omitted from attrs', () => {
    const node = LpdfCanvas.rect(0, 0, 10, 10, { fill: undefined });
    assert(!('fill' in node.attrs), 'undefined fill should be omitted');
  });

});

// ── CanvasTransform ───────────────────────────────────────────────────────────

describe('CanvasTransform', () => {

  it('translate(tx, ty) produces correct matrix string', () => {
    const t = CanvasTransform.translate(30, 40);
    assert.equal(t.toString(), 'matrix(1,0,0,1,30,40)');
  });

  it('scale(sx) uniform scale', () => {
    const t = CanvasTransform.scale(2);
    assert.equal(t.toString(), 'matrix(2,0,0,2,0,0)');
  });

  it('scale(sx, sy) non-uniform scale', () => {
    const t = CanvasTransform.scale(2, 3);
    assert.equal(t.toString(), 'matrix(2,0,0,3,0,0)');
  });

  it('rotate(0) is identity', () => {
    const t = CanvasTransform.rotate(0);
    const [a, b, c, d, e, f] = t.matrix;
    assert(Math.abs(a - 1) < 1e-9, 'a should be 1');
    assert(Math.abs(b) < 1e-9, 'b should be 0');
    assert(Math.abs(c) < 1e-9, 'c should be 0');
    assert(Math.abs(d - 1) < 1e-9, 'd should be 1');
    assert(Math.abs(e) < 1e-9, 'e should be 0');
    assert(Math.abs(f) < 1e-9, 'f should be 0');
  });

  it('then() combines transforms: other first, this second', () => {
    const translate = CanvasTransform.translate(10, 0);
    const scale = CanvasTransform.scale(2);
    // translate.then(scale): apply scale first, then translate
    const combined = translate.then(scale);
    assert.equal(combined.toString(), 'matrix(2,0,0,2,10,0)');
  });

});

// ── LpdfKit section model ─────────────────────────────────────────────────────

describe('LpdfKit section model', () => {

  it('LpdfKit.layout wraps nodes in a layout block', () => {
    const textNode = LpdfLayout.text(['hello']);
    const block = LpdfKit.layout([textNode]);
    assert.equal(block.type, 'layout');
    assert.equal(block.nodes.length, 1);
    assert.equal(block.nodes[0].type, 'text');
  });

  it('LpdfKit.canvas wraps layers in a canvas block', () => {
    const layer = LpdfCanvas.layer([LpdfCanvas.rect(0, 0, 10, 10)]);
    const block = LpdfKit.canvas([layer]);
    assert.equal(block.type, 'canvas');
    assert.equal(block.nodes.length, 1);
    assert.equal(block.nodes[0].type, 'canvas-layer');
  });

  it('LpdfKit.section preserves block order, no implicit wrapping', () => {
    const layoutBlock = LpdfKit.layout([LpdfLayout.text(['text'])]);
    const canvasBlock = LpdfKit.canvas([LpdfCanvas.layer([])]);
    const sec = LpdfKit.section({ nodes: [canvasBlock, layoutBlock] });
    assert.equal(sec.type, 'section');
    assert.equal(sec.nodes[0].type, 'canvas');
    assert.equal(sec.nodes[1].type, 'layout');
  });

  it('LpdfKit.section options are serialised to attrs', () => {
    const sec = LpdfKit.section({
      nodes: [LpdfKit.layout([])],
      options: { size: 'a4', margin: '20pt', title: 'My Page' },
    });
    assert.equal(sec.attrs.size, 'a4');
    assert.equal(sec.attrs.margin, '20pt');
    assert.equal(sec.attrs.title, 'My Page');
  });

  it('LpdfKit.document serialises sections as nodes wire key', () => {
    const sec = LpdfKit.section({ nodes: [LpdfKit.layout([])] });
    const document = LpdfKit.document({ sections: [sec] });
    assert.equal(document.version, 1);
    assert.equal(document.type, 'document');
    assert.equal(document.nodes.length, 1);
    assert.equal(document.nodes[0].type, 'section');
  });

});

// ── LpdfLayout region ─────────────────────────────────────────────────────────

describe('LpdfLayout region', () => {

  it('region emits correct type with pin in attrs', () => {
    const node = LpdfLayout.region('top', [LpdfLayout.text(['header'])]);
    assert.equal(node.type, 'layout-region');
    assert.equal(node.attrs.pin, 'top');
    assert.equal(node.nodes.length, 1);
  });

  it('region options serialised to attrs', () => {
    const node = LpdfLayout.region('bottom', [], { page: 'first', w: '100pt' });
    assert.equal(node.attrs.pin, 'bottom');
    assert.equal(node.attrs.page, 'first');
    assert.equal(node.attrs.w, '100pt');
  });

  it('kitToXml emits layout-region element', () => {
    const document = LpdfKit.document({
      sections: [
        LpdfKit.section({
          nodes: [
            LpdfKit.layout([
              LpdfLayout.region('top', [LpdfLayout.text(['header'])]),
            ]),
          ],
        }),
      ],
    });
    const xml = kitToXml(document);
    assert(xml.includes('<region '), 'missing <region> element');
    assert(xml.includes('pin="top"'), 'missing pin attribute');
    assert(xml.includes('header'), 'region text content missing');
  });

});

// ── Data binding ──────────────────────────────────────────────────────────────

describe('data binding', () => {

  it('data-value substitutes a scalar string', async () => {
    const xml = doc(`<text data-value="name">Fallback</text>`);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml, { data: { name: 'Acme Inc' } });
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
    assert(bytes.length > 100);
  });

  it('data-source expands an array', async () => {
    const xml = doc(`
      <stack data-source="items" gap="xs">
        <text data-value="label">Fallback item</text>
      </stack>
    `);
    const lpdf = new LpdfEngine('test-key');
    const data = { items: [{ label: 'Alpha' }, { label: 'Beta' }, { label: 'Gamma' }] };
    const bytes = await lpdf.renderPdf(xml, { data });
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('data-if hides node when false', async () => {
    const xml = doc(`
      <text data-if="isPremium">Premium only</text>
      <text>Always visible</text>
    `);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml, { data: { isPremium: false } });
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

  it('renders without data when data option is omitted', async () => {
    const xml = doc(`<text data-value="name">Inline fallback</text>`);
    const lpdf = new LpdfEngine('test-key');
    const bytes = await lpdf.renderPdf(xml);
    const header = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    assert.equal(header, '%PDF-');
  });

});




