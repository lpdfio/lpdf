"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.kitToXml = kitToXml;
// ── XML escaping ──────────────────────────────────────────────────────────────
function escapeAttr(s) {
    return s
        .replace(/&/g, '&amp;')
        .replace(/"/g, '&quot;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}
function escapeText(s) {
    return s
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}
// ── Attribute serialisation ───────────────────────────────────────────────────
function attrsStr(attrs) {
    return Object.entries(attrs)
        .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
        .join('');
}
// ── Tokens block ──────────────────────────────────────────────────────────────
const TOKEN_SCALES = ['space', 'grid', 'border', 'radius', 'width', 'text'];
function renderTokens(tokens, depth) {
    const pad = '  '.repeat(depth);
    const inner = '  '.repeat(depth + 1);
    const lines = [`${pad}<tokens>`];
    for (const scale of TOKEN_SCALES) {
        const row = tokens[scale];
        if (!row || Object.keys(row).length === 0)
            continue;
        const attrs = Object.entries(row)
            .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
            .join('');
        lines.push(`${inner}<${scale}${attrs}/>`);
    }
    const fonts = tokens.fonts;
    if (fonts && Object.keys(fonts).length > 0) {
        lines.push(`${inner}<fonts>`);
        const fontPad = '  '.repeat(depth + 2);
        for (const [name, def] of Object.entries(fonts)) {
            if ('builtin' in def && def.builtin) {
                lines.push(`${fontPad}<font name="${escapeAttr(name)}" builtin="${escapeAttr(def.builtin)}"/>`);
            }
            else if ('src' in def && def.src) {
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
function renderMeta(meta, depth) {
    const pad = '  '.repeat(depth);
    const attrs = Object.entries(meta)
        .filter((entry) => entry[1] !== undefined)
        .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
        .join('');
    return `${pad}<meta${attrs}/>`;
}
// ── Leaf nodes ────────────────────────────────────────────────────────────────
function renderSpan(node) {
    const content = node.children.map(escapeText).join('');
    const attrs = attrsStr(node.attrs);
    if (!content)
        return `<span${attrs}/>`;
    return `<span${attrs}>${content}</span>`;
}
function renderText(node, depth) {
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
function renderDivider(node, depth) {
    const pad = '  '.repeat(depth);
    return `${pad}<divider${attrsStr(node.attrs)}/>`;
}
// ── Container nodes ───────────────────────────────────────────────────────────
function renderContainer(node, depth) {
    const pad = '  '.repeat(depth);
    const attrs = attrsStr(node.attrs);
    if (node.children.length === 0) {
        return `${pad}<${node.type}${attrs}/>`;
    }
    const children = node.children.map(c => renderNode(c, depth + 1)).join('\n');
    return `${pad}<${node.type}${attrs}>\n${children}\n${pad}</${node.type}>`;
}
function renderNode(node, depth) {
    switch (node.type) {
        case 'text': return renderText(node, depth);
        case 'divider': return renderDivider(node, depth);
        default: return renderContainer(node, depth);
    }
}
// ── Page ──────────────────────────────────────────────────────────────────────
function renderPage(page, depth) {
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
function kitToXml(doc) {
    const { tokens, meta, ...docAttrs } = doc.attrs;
    const docAttrStr = Object.entries(docAttrs)
        .filter((entry) => typeof entry[1] === 'string')
        .map(([k, v]) => ` ${k}="${escapeAttr(v)}"`)
        .join('');
    const lines = ['<?xml version="1.0" encoding="UTF-8"?>', '<lpdf version="1">'];
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
