/**
 * kitToXml — serialise an LpdfDocument tree (produced by LpdfKit) back to an
 * lpdf XML string that can be passed directly to `LpdfEngine.renderPdf()` or
 * saved to disk.
 *
 * @example
 * ```ts
 * import { LpdfKit, kitToXml } from './dist/index.js';
 *
 * const doc = LpdfKit.document({ nodes: [...], options: { ... } });
 * const xml = kitToXml(doc);
 * console.log(xml);
 * ```
 */

import type {
  LpdfDocument,
  LpdfPageNode,
  LpdfNode,
  LpdfContainerNode,
  LpdfTextNode,
  LpdfSpanNode,
  LpdfDividerNode,
  LpdfTokens,
  LpdfFontDef,
  LpdfMeta,
} from './kit';

// ── XML escaping ──────────────────────────────────────────────────────────────

function escapeAttr(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function escapeText(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

// ── Attribute serialisation ───────────────────────────────────────────────────

function attrsStr(attrs: Record<string, string>): string {
  return Object.entries(attrs)
    .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
    .join('');
}

// ── Tokens block ──────────────────────────────────────────────────────────────

const TOKEN_SCALES = ['space', 'grid', 'border', 'radius', 'width', 'text'] as const;

function renderTokens(tokens: LpdfTokens, depth: number): string {
  const pad = '  '.repeat(depth);
  const inner = '  '.repeat(depth + 1);
  const lines: string[] = [`${pad}<tokens>`];

  for (const scale of TOKEN_SCALES) {
    const row = tokens[scale];
    if (!row || Object.keys(row).length === 0) continue;
    const attrs = Object.entries(row)
      .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
      .join('');
    lines.push(`${inner}<${scale}${attrs}/>`);
  }

  const fonts = tokens.fonts;
  if (fonts && Object.keys(fonts).length > 0) {
    lines.push(`${inner}<fonts>`);
    const fontPad = '  '.repeat(depth + 2);
    for (const [name, def] of Object.entries(fonts as Record<string, LpdfFontDef>)) {
      if ('builtin' in def && def.builtin) {
        lines.push(`${fontPad}<font name="${escapeAttr(name)}" builtin="${escapeAttr(def.builtin)}"/>`);
      } else if ('src' in def && def.src) {
        lines.push(`${fontPad}<font name="${escapeAttr(name)}" src="${escapeAttr(def.src)}"/>`);
      }
    }
    lines.push(`${inner}</fonts>`);
  }

  const colors = tokens.colors;
  if (colors && Object.keys(colors).length > 0) {
    lines.push(`${inner}<colors>`);
    const colorPad = '  '.repeat(depth + 2);
    for (const [name, value] of Object.entries(colors)) {
      lines.push(`${colorPad}<color name="${escapeAttr(name)}" value="${escapeAttr(value)}"/>`);
    }
    lines.push(`${inner}</colors>`);
  }

  lines.push(`${pad}</tokens>`);
  return lines.join('\n');
}

// ── Meta element ──────────────────────────────────────────────────────────────

function renderMeta(meta: LpdfMeta, depth: number): string {
  const pad = '  '.repeat(depth);
  const attrs = Object.entries(meta)
    .filter((entry): entry is [string, string] => entry[1] !== undefined)
    .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
    .join('');
  return `${pad}<meta${attrs}/>`;
}

// ── Leaf nodes ────────────────────────────────────────────────────────────────

function renderSpan(node: LpdfSpanNode): string {
  const content = node.children.map(escapeText).join('');
  const attrs = attrsStr(node.attrs);
  if (!content) return `<span${attrs}/>`;
  return `<span${attrs}>${content}</span>`;
}

function renderText(node: LpdfTextNode, depth: number): string {
  const pad = '  '.repeat(depth);
  const attrs = attrsStr(node.attrs);

  if (node.children.length === 0) {
    return `${pad}<text${attrs}/>`;
  }

  // Render children inline (mixed text + optional spans)
  const inner = node.children
    .map(c => (typeof c === 'string' ? escapeText(c) : renderSpan(c)))
    .join('');

  return `${pad}<text${attrs}>${inner}</text>`;
}

function renderDivider(node: LpdfDividerNode, depth: number): string {
  const pad = '  '.repeat(depth);
  return `${pad}<divider${attrsStr(node.attrs)}/>`;
}

// ── Container nodes ───────────────────────────────────────────────────────────

function renderContainer(node: LpdfContainerNode, depth: number): string {
  const pad = '  '.repeat(depth);
  const attrs = attrsStr(node.attrs);

  if (node.children.length === 0) {
    return `${pad}<${node.type}${attrs}/>`;
  }

  const children = node.children.map(c => renderNode(c, depth + 1)).join('\n');
  return `${pad}<${node.type}${attrs}>\n${children}\n${pad}</${node.type}>`;
}

function renderNode(node: LpdfNode, depth: number): string {
  switch (node.type) {
    case 'text':    return renderText(node, depth);
    case 'divider': return renderDivider(node, depth);
    default:        return renderContainer(node as LpdfContainerNode, depth);
  }
}

// ── Page ──────────────────────────────────────────────────────────────────────

function renderPage(page: LpdfPageNode, depth: number): string {
  const pad = '  '.repeat(depth);
  const attrs = attrsStr(page.attrs);

  if (page.children.length === 0) {
    return `${pad}<page${attrs}/>`;
  }

  const children = page.children.map(c => renderNode(c, depth + 1)).join('\n');
  return `${pad}<page${attrs}>\n${children}\n${pad}</page>`;
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Convert an `LpdfDocument` tree (built with `LpdfKit`) into an lpdf XML
 * string.
 *
 * The returned string can be passed directly to `LpdfEngine.renderPdf()` or
 * written to a `.xml` file.
 *
 * @param doc - The document tree returned by `LpdfKit.document(...)`.
 * @returns A well-formed XML string with an `<?xml ...?>` declaration.
 */
export function kitToXml(doc: LpdfDocument): string {
  const { tokens, meta, ...docAttrs } = doc.attrs as {
    tokens?: LpdfTokens;
    meta?:   LpdfMeta;
    [key: string]: unknown;
  };

  const docAttrStr = Object.entries(docAttrs)
    .filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
    .join('');

  const lines: string[] = ['<?xml version="1.0" encoding="UTF-8"?>', '<lpdf version="1">'];

  if (tokens && Object.keys(tokens).length > 0) {
    lines.push(renderTokens(tokens, 1));
  }

  lines.push(`  <document${docAttrStr}>`);

  if (meta && Object.keys(meta).length > 0) {
    lines.push(renderMeta(meta, 2));
  }

  lines.push('    <pages>');
  for (const page of doc.children) {
    lines.push(renderPage(page, 3));
  }
  lines.push('    </pages>');

  lines.push('  </document>');
  lines.push('</lpdf>');

  return lines.join('\n');
}
