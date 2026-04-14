"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.loadFonts = loadFonts;
exports.buildPdf = buildPdf;
exports.parseHex = parseHex;
/**
 * Shared render-tree types and pdf-lib drawing logic used by both the Node.js
 * and browser entry points.  No Node built-ins (no `node:fs`, no `require`).
 */
const pdf_lib_1 = require("pdf-lib");
// ── Standard font name → pdf-lib enum ────────────────────────────────────────
const BUILTIN_FONTS = {
    'Helvetica': pdf_lib_1.StandardFonts.Helvetica,
    'Helvetica-Bold': pdf_lib_1.StandardFonts.HelveticaBold,
    'Helvetica-Oblique': pdf_lib_1.StandardFonts.HelveticaOblique,
    'Helvetica-BoldOblique': pdf_lib_1.StandardFonts.HelveticaBoldOblique,
    'Times-Roman': pdf_lib_1.StandardFonts.TimesRoman,
    'Times-Bold': pdf_lib_1.StandardFonts.TimesRomanBold,
    'Times-Italic': pdf_lib_1.StandardFonts.TimesRomanItalic,
    'Times-BoldItalic': pdf_lib_1.StandardFonts.TimesRomanBoldItalic,
    'Courier': pdf_lib_1.StandardFonts.Courier,
    'Courier-Bold': pdf_lib_1.StandardFonts.CourierBold,
    'Courier-Oblique': pdf_lib_1.StandardFonts.CourierOblique,
    'Courier-BoldOblique': pdf_lib_1.StandardFonts.CourierBoldOblique,
    'Symbol': pdf_lib_1.StandardFonts.Symbol,
    'ZapfDingbats': pdf_lib_1.StandardFonts.ZapfDingbats,
};
// ── Font loading ──────────────────────────────────────────────────────────────
function collectFontNames(nodes, out) {
    for (const node of nodes) {
        if (node.type === 'text') {
            out.add(node.font);
        }
        else if (node.type === 'box') {
            collectFontNames(node.children, out);
        }
        else if (node.type === 'link') {
            collectFontNames(node.children, out);
        }
    }
}
/**
 * Embed all fonts referenced by the render tree into the PDF document.
 *
 * @param srcFallback  Optional callback to load font bytes by path (Node only).
 *                     In the browser this is left undefined; callers must
 *                     supply all custom font bytes via `providedBytes`.
 */
async function loadFonts(doc, tree, providedBytes, srcFallback) {
    const names = new Set();
    for (const page of tree.pages) {
        collectFontNames(page.nodes, names);
    }
    // Always embed Helvetica so the watermark path always has a font.
    names.add('Helvetica');
    const fontMap = new Map();
    for (const name of names) {
        const def = tree.meta.fonts?.[name];
        if (def?.src) {
            let bytes = providedBytes[name];
            if (!bytes && srcFallback) {
                bytes = srcFallback(def.src);
            }
            if (!bytes) {
                throw new Error(`Font "${name}" has src="${def.src}" but no bytes were provided. ` +
                    `Pass font bytes via the fontBytes option.`);
            }
            fontMap.set(name, await doc.embedFont(bytes));
        }
        else {
            const builtinName = def?.builtin ?? name;
            const stdFont = BUILTIN_FONTS[builtinName] ?? pdf_lib_1.StandardFonts.Helvetica;
            fontMap.set(name, await doc.embedFont(stdFont));
        }
    }
    return fontMap;
}
// ── PDF document assembly ─────────────────────────────────────────────────────
function addUriAnnotation(doc, page, rect, url) {
    const annotRef = doc.context.register(doc.context.obj({
        Type: pdf_lib_1.PDFName.of('Annot'),
        Subtype: pdf_lib_1.PDFName.of('Link'),
        Rect: rect,
        Border: [0, 0, 0],
        A: doc.context.obj({
            Type: pdf_lib_1.PDFName.of('Action'),
            S: pdf_lib_1.PDFName.of('URI'),
            URI: pdf_lib_1.PDFString.of(url),
        }),
    }));
    const existing = page.node.lookupMaybe(pdf_lib_1.PDFName.of('Annots'), pdf_lib_1.PDFArray);
    if (existing) {
        existing.push(annotRef);
    }
    else {
        page.node.set(pdf_lib_1.PDFName.of('Annots'), doc.context.obj([annotRef]));
    }
}
async function buildPdf(tree, providedBytes, srcFallback) {
    const doc = await pdf_lib_1.PDFDocument.create();
    if (tree.meta.title)
        doc.setTitle(tree.meta.title);
    if (tree.meta.author)
        doc.setAuthor(tree.meta.author);
    if (tree.meta.subject)
        doc.setSubject(tree.meta.subject);
    if (tree.meta.keywords?.length)
        doc.setKeywords(tree.meta.keywords);
    if (tree.meta.creator)
        doc.setCreator(tree.meta.creator);
    doc.setProducer('lpdf.io');
    const fontMap = await loadFonts(doc, tree, providedBytes, srcFallback);
    for (const pageData of tree.pages) {
        const page = doc.addPage([pageData.width, pageData.height]);
        if (pageData.background) {
            page.drawRectangle({
                x: 0, y: 0,
                width: pageData.width,
                height: pageData.height,
                color: parseHex(pageData.background),
                borderWidth: 0,
            });
        }
        for (const node of pageData.nodes) {
            drawNode(doc, page, node, fontMap);
        }
        if (tree.watermark) {
            const wFont = fontMap.get('Helvetica');
            const wText = tree.watermark.text;
            const wSize = 8;
            const wPad = 4; // fixed distance from page edge, independent of margin
            const textW = wFont.widthOfTextAtSize(wText, wSize);
            const wX = pageData.width - wPad - textW;
            const wY = pageData.height - wPad - wSize;
            page.drawText(wText, {
                x: wX,
                y: wY,
                font: wFont,
                size: wSize,
                color: parseHex('#aaaaaa'),
            });
            if (tree.watermark.url) {
                addUriAnnotation(doc, page, [wX, wY, wX + textW, wY + wSize], tree.watermark.url);
            }
        }
    }
    return doc.save();
}
// ── Drawing ───────────────────────────────────────────────────────────────────
function drawNode(doc, page, node, fonts) {
    const pageH = page.getHeight();
    if (node.type === 'box') {
        const hasFill = !!node.fill;
        const hasBorder = node.border_width > 0 && !!node.border_color;
        const pdfY = pageH - node.y - node.height;
        if (hasFill || hasBorder) {
            page.drawRectangle({
                x: node.x,
                y: pdfY,
                width: node.width,
                height: node.height,
                color: hasFill ? parseHex(node.fill) : undefined,
                borderColor: hasBorder ? parseHex(node.border_color) : undefined,
                borderWidth: hasBorder ? node.border_width : 0,
                // pdf-lib has no native border-radius; radius is not currently rendered.
            });
        }
        for (const child of node.children) {
            drawNode(doc, page, child, fonts);
        }
    }
    else if (node.type === 'line') {
        page.drawLine({
            start: { x: node.x1, y: pageH - node.y1 },
            end: { x: node.x2, y: pageH - node.y2 },
            thickness: node.thickness,
            color: parseHex(node.color),
        });
    }
    else if (node.type === 'text') {
        const font = fonts.get(node.font) ?? fonts.get('Helvetica');
        // node.x is an alignment anchor; offset by the real glyph width so the
        // renderer rather than the layout engine handles font-metric precision.
        const textW = font.widthOfTextAtSize(node.content, node.size);
        let drawX;
        if (node.text_align === 'right') {
            drawX = node.x - textW;
        }
        else if (node.text_align === 'center') {
            drawX = node.x - textW / 2;
        }
        else {
            drawX = node.x; // 'left' — anchor is already the left edge
        }
        // render-tree y = top of text block; pdf-lib y = baseline
        const pdfY = pageH - node.y - node.size;
        page.drawText(node.content, {
            x: drawX,
            y: pdfY,
            font,
            size: node.size,
            color: parseHex(node.color),
        });
    }
    else if (node.type === 'link') {
        for (const child of node.children) {
            drawNode(doc, page, child, fonts);
        }
        addUriAnnotation(doc, page, [
            node.x,
            pageH - node.y - node.height,
            node.x + node.width,
            pageH - node.y,
        ], node.url);
    }
}
// ── Colour helper ─────────────────────────────────────────────────────────────
function parseHex(hex) {
    let h = hex.replace('#', '');
    if (h.length === 3) {
        h = h.split('').map(c => c + c).join('');
    }
    return (0, pdf_lib_1.rgb)(parseInt(h.slice(0, 2), 16) / 255, parseInt(h.slice(2, 4), 16) / 255, parseInt(h.slice(4, 6), 16) / 255);
}
