#!/usr/bin/env node
// Builds the PDF viewer page of the demo on lpdf.io from the viewer of the VS Code extension, so the two are one:
//
//   src/vscode/media/viewer/        the vendored PDF.js viewer, and the files that make it Lpdf's (lpdf-host.mjs,
//                                   lpdf-toolbar.mjs, lpdf-theme.css, ...)
//   src/vscode/dist/viewer-html.js  the function that builds the page the extension shows it in; compiled by
//                                   `npm run build` in src/vscode
//
// What it writes, in <pages>/ui/demo/viewer/ (the demo copies that folder to lpdf.io):
//
//   index.html       the page, from the same function as the extension's, for a plain web page: the CSP names 'self'
//                    in place of a webview's source, and the files are addressed relative to the page
//   viewer-shim.js   stands in for what VS Code gives the page: acquireVsCodeApi, which here posts to the demo that
//                    holds the iframe, and the theme class VS Code puts on the body, which here follows the demo's
//   build/, web/, lpdf-*.mjs, lpdf-theme.css, LICENSE   copied as they are
//
// The demo sends the page the messages the extension host sends (showLoading, updatePdf, showError) and receives
// the ones the page sends (ready, download, log). See media/viewer/lpdf-host.mjs.
//
// Usage: node scripts/build-demo-viewer.mjs            write the folder
//        node scripts/build-demo-viewer.mjs --check    exit 1 if the folder is not what this would write
//
// The pages checkout is found at ../codesense/pages/lpdf, or at $PAGES_DIR or --pages <dir>.

import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..');
const source = path.join(repo, 'src', 'vscode', 'media', 'viewer');
const builtHtml = path.join(repo, 'src', 'vscode', 'dist', 'viewer-html.js');

const args = process.argv.slice(2);
const check = args.includes('--check');
const pagesArg = args.includes('--pages') ? args[args.indexOf('--pages') + 1] : undefined;
const pagesDir = path.resolve(pagesArg ?? process.env.PAGES_DIR ?? path.join(repo, '..', 'codesense', 'pages', 'lpdf'));
const outDir = path.join(pagesDir, 'ui', 'demo', 'viewer');

/** Files of the extension's viewer folder that are for the extension's maintainers, not for a page. */
const NOT_COPIED = new Set(['README.md', 'vendored.json']);
/** The diff view's scripts, which the demo has no use for. */
const NOT_COPIED_PATTERN = /^lpdf-diff-/;

function fail(message) {
    console.error(`build-demo-viewer: ${message}`);
    process.exit(2);
}

if (!fs.existsSync(path.join(pagesDir, 'ui'))) fail(`the pages checkout is not at ${pagesDir}`);
if (!fs.existsSync(builtHtml)) fail(`${path.relative(repo, builtHtml)} is missing: run npm run build in src/vscode`);

const { buildViewerHtml } = createRequire(import.meta.url)(builtHtml);

// The page is built for no fixed address: everything is relative to it, so it works wherever the demo is served.
// The nonce is fixed because the page is a file; the CSP's 'self' is what lets its scripts run.
const html = buildViewerHtml({
    viewerHtml: fs.readFileSync(path.join(source, 'web', 'viewer.html'), 'utf8'),
    viewerRoot: './',
    cspSource: "'self'",
    nonce: 'lpdf-demo',
}).replace('<script type="module"', '<script src="./viewer-shim.js"></script>\n    <script type="module"');
if (!html.includes('viewer-shim.js')) fail('could not place the shim in the page: the module script of buildViewerHtml has changed');

const shim = `// Stands in, for the viewer page of the demo on lpdf.io, for what VS Code gives a webview. Written by
// scripts/build-demo-viewer.mjs of the lpdf repository; do not edit it here.
//
// lpdf-host.mjs talks to the extension host through acquireVsCodeApi(). Here the page is in an iframe of the
// demo, so its messages go to the page that holds it, which answers with the same ones the extension host sends.
//
// The page can also show a PDF by itself: index.html?pdf=/docs/examples/invoice/document.pdf fetches that file, from this
// site only, and shows it when the viewer is ready, and its Save button saves it. The docs use this for their examples.
(function () {
    var parentWindow = window.parent;
    var pdfPath = new URLSearchParams(location.search).get('pdf');
    // A path on this site, and not an address that another one could be reached by.
    var standalone = pdfPath && pdfPath.charAt(0) === '/' && pdfPath.charAt(1) !== '/' && pdfPath.indexOf('\\\\') < 0 ? pdfPath : null;

    function toBase64(bytes) {
        var binary = '';
        for (var i = 0; i < bytes.length; i += 0x8000) { binary += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000)); }
        return btoa(binary);
    }
    function showStandalonePdf() {
        fetch(standalone).then(function (response) {
            if (!response.ok) { throw new Error(response.status + ' ' + response.statusText); }
            return response.arrayBuffer();
        }).then(function (buffer) {
            window.postMessage({ type: 'updatePdf', pdfBase64: toBase64(new Uint8Array(buffer)), filename: standalone.split('/').pop(), zoom: 'fit' }, location.origin);
        }).catch(function (error) {
            window.postMessage({ type: 'showError', message: 'Could not load ' + standalone + ': ' + error.message }, location.origin);
        });
    }
    function saveStandalonePdf(message) {
        var binary = atob(message.pdfBase64), bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) { bytes[i] = binary.charCodeAt(i); }
        var link = document.createElement('a');
        link.href = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }));
        link.download = message.filename || 'document.pdf';
        link.click();
        URL.revokeObjectURL(link.href);
    }

    window.acquireVsCodeApi = function () {
        return {
            postMessage: function (message) {
                if (standalone && message && message.type === 'ready') { showStandalonePdf(); }
                else if (standalone && message && message.type === 'download') { saveStandalonePdf(message); }
                parentWindow.postMessage(message, location.origin);
            },
            getState: function () { return undefined; },
            setState: function () {},
        };
    };

    // VS Code puts vscode-light or vscode-dark on the body, and lpdf-host.mjs reads it to pick the viewer's colours.
    // Here the demo's own theme, the data-theme of its <html>, says which.
    function demoIsDark() {
        try { return parentWindow.document.documentElement.getAttribute('data-theme') === 'dark'; } catch (e) { return false; }
    }
    function applyTheme() {
        var dark = demoIsDark();
        document.body.classList.toggle('vscode-dark', dark);
        document.body.classList.toggle('vscode-light', !dark);
    }
    document.addEventListener('DOMContentLoaded', function () {
        applyTheme();
        try {
            new MutationObserver(applyTheme).observe(parentWindow.document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
        } catch (e) { /* a parent from another origin cannot be watched; the theme stays as it was */ }
    });
})();
`;

/** @returns {Map<string, Buffer>} every file of the folder, by path relative to it, with / between folders */
function expected() {
    const files = new Map();
    const walk = (rel = '') => {
        for (const entry of fs.readdirSync(path.join(source, rel), { withFileTypes: true })) {
            const r = rel ? `${rel}/${entry.name}` : entry.name;
            if (!rel && (NOT_COPIED.has(entry.name) || NOT_COPIED_PATTERN.test(entry.name))) continue;
            if (entry.isDirectory()) walk(r);
            else files.set(r, fs.readFileSync(path.join(source, r)));
        }
    };
    walk();
    files.set('index.html', Buffer.from(html, 'utf8'));
    files.set('viewer-shim.js', Buffer.from(shim, 'utf8'));
    return files;
}

function present(rel = '') {
    const dir = path.join(outDir, rel);
    if (!fs.existsSync(dir)) return [];
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
        const r = rel ? `${rel}/${entry.name}` : entry.name;
        return entry.isDirectory() ? present(r) : [r];
    });
}

const files = expected();
let changes = 0;
const note = (what, rel) => { changes++; console.log(`  ${what} ${path.relative(repo, path.join(outDir, rel)).replace(/\\/g, '/')}`); };
for (const [rel, bytes] of files) {
    const file = path.join(outDir, rel);
    const there = fs.existsSync(file) ? fs.readFileSync(file) : undefined;
    if (there && there.equals(bytes)) continue;
    note(there ? 'update' : 'add   ', rel);
    if (!check) {
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, bytes);
    }
}
for (const rel of present()) {
    if (files.has(rel)) continue;
    note('remove', rel);
    if (!check) fs.rmSync(path.join(outDir, rel));
}
console.log(changes === 0 ? 'the demo viewer: up to date' : `the demo viewer: ${changes} file${changes === 1 ? '' : 's'} ${check ? 'differ' : 'written'}`);
if (check && changes > 0) {
    console.error('Run: node scripts/build-demo-viewer.mjs');
    process.exit(1);
}
