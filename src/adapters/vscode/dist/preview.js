"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.previewPdf = previewPdf;
exports.renderForUri = renderForUri;
const vscode = __importStar(require("vscode"));
const path = __importStar(require("node:path"));
const engine_1 = require("./engine");
let previewPanel;
let previewUri;
let previewBytes;
let renderGeneration = 0;
let lastRenderedContent;
// Ready-handshake state: the webview signals 'ready' once its message listener is registered.
// Any message sent before that point is queued and flushed on 'ready'.
let _webviewReady = false;
let _pendingMessage;
/** Send a message to the webview. Queues it if the webview hasn't signalled ready yet. */
function postToWebview(msg) {
    if (!previewPanel) {
        return;
    }
    if (_webviewReady) {
        previewPanel.webview.postMessage(msg).then(() => { console.log(`[lpdf] → webview: ${msg.type}`); }, (err) => { console.error('[lpdf] postMessage failed:', err); });
    }
    else {
        // Queue — newer message always supersedes a queued one (e.g. updatePdf replaces showLoading).
        console.log(`[lpdf] webview not ready — queuing: ${msg.type} (replaces: ${_pendingMessage?.type ?? 'none'})`);
        _pendingMessage = msg;
    }
}
/**
 * Open or reveal the preview panel, then render the given URI.
 * Called by the "Preview PDF" command and the CodeLens button.
 * uri is passed when invoked from the Explorer context menu; falls back to the active editor.
 */
async function previewPdf(context, uri) {
    const targetUri = uri ?? vscode.window.activeTextEditor?.document.uri;
    if (!targetUri) {
        return;
    }
    const doc = await vscode.workspace.openTextDocument(targetUri);
    if (doc.languageId !== 'xml') {
        vscode.window.showErrorMessage('lpdf: Active file is not an XML document.');
        return;
    }
    ensurePanel(context);
    await doRender(context, targetUri);
}
/**
 * Re-render the panel for a URI if the panel is already open.
 * reason 'switch': only re-renders if the URI differs from what is currently shown.
 * reason 'save':   only re-renders if the document content changed since the last render.
 */
async function renderForUri(context, uri, reason) {
    if (!previewPanel) {
        return;
    }
    if (reason === 'switch' && previewUri?.toString() === uri.toString()) {
        return; // same file already showing — nothing to do
    }
    if (reason === 'save') {
        const doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
        if (doc && doc.getText() === lastRenderedContent) {
            return; // content unchanged since last render
        }
    }
    await doRender(context, uri);
}
function ensurePanel(context) {
    if (previewPanel) {
        previewPanel.reveal(vscode.ViewColumn.Beside, true);
        return;
    }
    const mediaUri = vscode.Uri.joinPath(context.extensionUri, 'media');
    previewPanel = vscode.window.createWebviewPanel('lpdfPreview', 'lpdf Preview', { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true }, {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [mediaUri],
    });
    // Build the webview HTML once — pdf.js is loaded at panel creation and reused across renders.
    const pdfJsUri = previewPanel.webview.asWebviewUri(vscode.Uri.joinPath(mediaUri, 'pdf.min.mjs'));
    const workerUri = previewPanel.webview.asWebviewUri(vscode.Uri.joinPath(mediaUri, 'pdf.worker.min.mjs'));
    const cspSource = previewPanel.webview.cspSource;
    previewPanel.webview.html = buildStaticHtml(pdfJsUri, workerUri, cspSource);
    previewPanel.onDidDispose(() => {
        console.log('[lpdf] preview panel disposed');
        previewPanel = undefined;
        previewUri = undefined;
        previewBytes = undefined;
        lastRenderedContent = undefined;
        _webviewReady = false;
        _pendingMessage = undefined;
    });
    previewPanel.webview.onDidReceiveMessage(async (msg) => {
        // Relay console messages from the webview to the extension host debug console.
        if (msg.type === 'log') {
            if (msg.level === 'error') {
                console.error('[lpdf webview]', msg.message);
            }
            else {
                console.log('[lpdf webview]', msg.message);
            }
            return;
        }
        // Webview signals that its message listener is fully registered.
        if (msg.type === 'ready') {
            console.log('[lpdf] webview ready — flushing pending:', _pendingMessage?.type ?? 'none');
            _webviewReady = true;
            if (_pendingMessage && previewPanel) {
                const pending = _pendingMessage;
                _pendingMessage = undefined;
                previewPanel.webview.postMessage(pending).then(() => { console.log(`[lpdf] → webview (flushed): ${pending.type}`); }, (err) => { console.error('[lpdf] flush postMessage failed:', err); });
            }
            return;
        }
        if (msg.type !== 'download' || !previewBytes || !previewUri) {
            return;
        }
        const xmlDir = path.dirname(previewUri.fsPath);
        const defaultName = path.basename(previewUri.fsPath, '.xml') + '.pdf';
        const saveUri = await vscode.window.showSaveDialog({
            defaultUri: vscode.Uri.file(path.join(xmlDir, defaultName)),
            filters: { 'PDF': ['pdf'] },
        });
        if (!saveUri) {
            return;
        }
        await vscode.workspace.fs.writeFile(saveUri, previewBytes);
        vscode.window.showInformationMessage(`lpdf: Saved ${path.basename(saveUri.fsPath)}`);
    }, undefined, context.subscriptions);
}
async function doRender(context, uri) {
    if (!previewPanel) {
        return;
    }
    const isNewFile = previewUri?.toString() !== uri.toString();
    previewUri = uri;
    const generation = ++renderGeneration;
    const file = path.basename(uri.fsPath);
    console.log(`[lpdf] doRender start  gen=${generation} file=${file} isNewFile=${isNewFile} webviewReady=${_webviewReady}`);
    const doc = await vscode.workspace.openTextDocument(uri);
    if (generation !== renderGeneration || !previewPanel) {
        console.log(`[lpdf] doRender stale after openTextDocument gen=${generation}`);
        return;
    }
    const xml = doc.getText();
    const xmlDir = path.dirname(uri.fsPath);
    const title = path.basename(uri.fsPath, '.xml') + ' — Preview';
    previewPanel.title = title;
    // Show loading overlay. If the webview isn't ready yet this gets queued and
    // will be superseded by updatePdf/showError when that arrives.
    postToWebview({ type: 'showLoading' });
    const licenseKey = vscode.workspace.getConfiguration('lpdf').get('licenseKey', '');
    try {
        console.log(`[lpdf] doRender WASM start gen=${generation}`);
        const bytes = await (0, engine_1.renderPdf)(xml, licenseKey, xmlDir);
        console.log(`[lpdf] doRender WASM done  gen=${generation} bytes=${bytes.byteLength}`);
        if (generation !== renderGeneration || !previewPanel) {
            console.log(`[lpdf] doRender stale after WASM gen=${generation} current=${renderGeneration}`);
            return;
        }
        lastRenderedContent = xml;
        previewBytes = bytes;
        const pdfBase64 = Buffer.from(bytes).toString('base64');
        const filename = path.basename(uri.fsPath, '.xml') + '.pdf';
        // Only pass zoom/scroll for new files; re-renders of the same file preserve the webview's state.
        const msg = { type: 'updatePdf', pdfBase64, filename, watermarked: !licenseKey };
        if (isNewFile) {
            msg.zoom = 'fit';
            msg.scrollX = 0;
            msg.scrollY = 0;
        }
        postToWebview(msg);
    }
    catch (e) {
        if (generation !== renderGeneration || !previewPanel) {
            console.log(`[lpdf] doRender stale after error gen=${generation}`);
            return;
        }
        previewBytes = undefined;
        const message = e instanceof engine_1.LpdfRenderError ? e.message : String(e);
        console.error(`[lpdf] doRender error gen=${generation}:`, message);
        postToWebview({ type: 'showError', message });
        vscode.window.showErrorMessage(`lpdf: ${message}`);
    }
}
function buildStaticHtml(pdfJsUri, workerUri, cspSource) {
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy"
        content="default-src 'none';
                 script-src ${cspSource} 'unsafe-inline';
                 worker-src ${cspSource} blob:;
                 style-src 'unsafe-inline';">
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    html, body { height: 100%; display: flex; flex-direction: column; background: #525659; }
    #viewer {
      flex: 1; overflow: auto;
      padding: 16px 12px; gap: 12px;
    }
    #viewer-inner {
      display: flex; flex-direction: column; align-items: center;
      min-width: min-content; min-height: 100%;
      gap: 12px; padding-bottom: 16px;
    }
    canvas { box-shadow: 0 2px 10px rgba(0,0,0,.55); display: block; }
    #toolbar {
      flex-shrink: 0; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center;
      padding: 5px 10px; background: #3c3c3c; border-bottom: 1px solid #252526;
      font: 12px/1 sans-serif; color: #ccc; user-select: none;
    }
    #toolbar-left  { display: flex; align-items: center; gap: 6px; justify-content: flex-start; }
    #toolbar-center { display: flex; align-items: center; gap: 6px; justify-content: center; }
    #toolbar-right { display: flex; align-items: center; gap: 6px; justify-content: flex-end; }
    #toolbar button {
      background: #4a4a4a; color: #ccc; border: 1px solid #555;
      padding: 3px 8px; cursor: pointer; border-radius: 3px; font-size: 12px;
    }
    #toolbar button:hover { background: #5e5e5e; color: #fff; }
    #toolbar button.primary { background: #0e639c; color: #fff; border-color: #0e639c; }
    #toolbar button.primary:hover { background: #1177bb; }
    #zoom-select {
      background: #4a4a4a; color: #ccc; border: 1px solid #555;
      padding: 3px 6px; border-radius: 3px; font-size: 12px; cursor: pointer;
      min-width: 62px;
    }
    /* Loading overlay sits above everything; visible by default for first render */
    #loading-overlay {
      position: fixed; inset: 0; z-index: 200;
      display: flex; align-items: center; justify-content: center;
      background: #525659; color: #ccc; font: 14px/1.6 sans-serif;
    }
    #loading-overlay.hidden { display: none; }
    #props-overlay {
      display: none; position: fixed; inset: 0;
      background: rgba(0,0,0,.5); z-index: 100;
      align-items: center; justify-content: center;
    }
    #props-overlay.open { display: flex; }
    #props-dialog {
      background: #2d2d2d; border: 1px solid #454545; border-radius: 4px;
      min-width: 360px; max-width: 520px; width: 90%;
      font: 12px/1.5 sans-serif; color: #ccc;
      display: flex; flex-direction: column;
    }
    #props-title {
      padding: 10px 14px; font-weight: 600; font-size: 13px;
      border-bottom: 1px solid #454545; color: #e0e0e0;
    }
    #props-table { width: 100%; border-collapse: collapse; padding: 6px 0; }
    #props-table td { padding: 4px 14px; vertical-align: top; }
    #props-table td:first-child { color: #aaa; white-space: nowrap; width: 40%; }
    #props-table tr:hover td { background: #383838; }
    #props-footer {
      padding: 8px 14px; border-top: 1px solid #454545;
      display: flex; justify-content: flex-end;
    }
    #watermark-bar {
      display: none; flex-shrink: 0; background: #5c3d00; color: #ffc107;
      font: 11px/1.4 sans-serif; padding: 5px 10px;
      text-align: center; border-bottom: 1px solid #7a5200;
    }
  </style>
</head>
<body>
  <div id="watermark-bar">Preview only \u00b7 license key is needed to export</div>
  <div id="toolbar">
    <div id="toolbar-left">
      <button class="primary" id="btn-download">Export PDF</button>
    </div>
    <div id="toolbar-center">
      <button id="btn-zoom-out" title="Zoom out">&minus;</button>
      <select id="zoom-select" title="Zoom level">
        <option value="fit">Fit</option>
        <option value="fill">Fill</option>
        <option value="100pct">100%</option>
      </select>
      <button id="btn-zoom-in" title="Zoom in">+</button>
    </div>
    <div id="toolbar-right">
      <button id="btn-props">Properties</button>
    </div>
  </div>
  <div id="props-overlay">
    <div id="props-dialog">
      <div id="props-title">Document Properties</div>
      <table id="props-table"></table>
      <div id="props-footer"><button id="btn-props-close">Close</button></div>
    </div>
  </div>
  <div id="viewer"><div id="viewer-inner"></div></div>
  <!-- Visible by default so first render shows "Rendering…" without needing a message -->
  <div id="loading-overlay">Rendering\u2026</div>
  <script>
    // Polyfill for Map.prototype.getOrInsertComputed (TC39 Stage 2, not yet in stable V8).
    // Required here for pdf.min.mjs running in the main thread.
    // pdf.worker.min.mjs has its own copy for the worker thread — both are needed
    // because the main thread and worker thread are separate JS execution contexts.
    if (!Map.prototype.getOrInsertComputed) {
      Map.prototype.getOrInsertComputed = function(key, callbackfn) {
        if (!this.has(key)) { this.set(key, callbackfn(key)); }
        return this.get(key);
      };
    }
    const vscodeApi = acquireVsCodeApi();
  </script>
  <script type="module">
    import * as pdfjsLib from '${pdfJsUri}';
    pdfjsLib.GlobalWorkerOptions.workerSrc = '${workerUri}';

    /** Relay a log message to the extension host debug console. */
    function log(level, ...args) {
      const message = args.map(a => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(' ');
      vscodeApi.postMessage({ type: 'log', level, message });
    }

    let pdfDoc       = null;
    let currentScale = 1;
    let fitScale     = 1;
    let fillScale    = 1;
    let filename     = '';
    let totalBytes   = 0;

    const overlay      = document.getElementById('loading-overlay');
    const inner        = document.getElementById('viewer-inner');
    const viewer       = document.getElementById('viewer');
    const watermarkBar = document.getElementById('watermark-bar');

    function showLoading() {
      log('info', 'showLoading');
      overlay.style.color = '#ccc';
      overlay.textContent = 'Rendering\u2026';
      overlay.classList.remove('hidden');
    }

    function hideLoading() {
      overlay.classList.add('hidden');
    }

    function showError(message) {
      overlay.style.color = '#e88';
      overlay.textContent = 'Error: ' + message;
      overlay.classList.remove('hidden');
    }

    async function renderAll(scale) {
      inner.innerHTML = '';
      for (let p = 1; p <= pdfDoc.numPages; p++) {
        const page   = await pdfDoc.getPage(p);
        const vp     = page.getViewport({ scale });
        const canvas = document.createElement('canvas');
        canvas.width  = vp.width;
        canvas.height = vp.height;
        inner.appendChild(canvas);
        await page.render({ canvasContext: canvas.getContext('2d'), viewport: vp }).promise;
      }
    }

    async function loadPdf(pdfBase64, newFilename, newWatermarked, zoomOpt, scrollXOpt, scrollYOpt) {
      filename = newFilename;
      watermarkBar.style.display = newWatermarked ? '' : 'none';

      const bin = atob(pdfBase64);
      const buf = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
      totalBytes = buf.byteLength;

      pdfDoc = await pdfjsLib.getDocument({ data: buf }).promise;
      const firstPage = await pdfDoc.getPage(1);
      const natural   = firstPage.getViewport({ scale: 1 });
      // Subtract padding (2×12px) and scrollbar allowance (17px) to avoid a horizontal scrollbar at fit.
      fitScale  = Math.max(0.25, (viewer.clientWidth - 24 - 17) / natural.width);
      fillScale = Math.max(0.25, viewer.clientHeight / natural.height);

      if (zoomOpt !== undefined) {
        // New file — apply supplied zoom and scroll.
        if (zoomOpt === 'fit') {
          currentScale = fitScale;
          document.getElementById('zoom-select').value = 'fit';
        } else if (zoomOpt === 'fill') {
          currentScale = fillScale;
          document.getElementById('zoom-select').value = 'fill';
        } else if (zoomOpt === '100pct') {
          currentScale = 1;
          document.getElementById('zoom-select').value = '100pct';
        } else {
          currentScale = zoomOpt;
          updateLabel();
        }
        await renderAll(currentScale);
        viewer.scrollLeft = scrollXOpt ?? 0;
        viewer.scrollTop  = scrollYOpt ?? 0;
      } else {
        // Re-render same file — restore content-relative position via page anchor:
        // record which canvas is at the top of the viewport and how far into it,
        // then after re-rendering scroll back to that same page + fractional offset.
        let anchorIdx = 0;
        let anchorFraction = 0;
        const canvasBefore = Array.from(inner.children);
        if (canvasBefore.length > 0) {
          const viewerTop = viewer.getBoundingClientRect().top;
          for (let i = 0; i < canvasBefore.length; i++) {
            const r = canvasBefore[i].getBoundingClientRect();
            if (r.bottom > viewerTop) {
              anchorIdx = i;
              anchorFraction = Math.max(0, Math.min(1, (viewerTop - r.top) / r.height));
              break;
            }
          }
        }
        const xScrollable = viewer.scrollWidth - viewer.clientWidth;
        const anchorScrollXFraction = xScrollable > 0 ? viewer.scrollLeft / xScrollable : 0;

        await renderAll(currentScale);

        const canvasAfter = Array.from(inner.children);
        if (canvasAfter[anchorIdx]) {
          const c = canvasAfter[anchorIdx];
          const viewerTop2 = viewer.getBoundingClientRect().top;
          const cTop = c.getBoundingClientRect().top - viewerTop2 + viewer.scrollTop;
          viewer.scrollTop = Math.max(0, cTop + anchorFraction * c.height);
        }
        const xScrollable2 = viewer.scrollWidth - viewer.clientWidth;
        viewer.scrollLeft = xScrollable2 > 0 ? Math.round(anchorScrollXFraction * xScrollable2) : 0;
      }

      hideLoading();
    }

    // Receive messages from the extension host.
    window.addEventListener('message', async event => {
      const msg = event.data;
      log('info', 'message received:', msg.type);
      if (msg.type === 'showLoading') {
        showLoading();
      } else if (msg.type === 'updatePdf') {
        try {
          log('info', 'loadPdf start filename=' + msg.filename + ' bytes=' + msg.pdfBase64.length);
          await loadPdf(msg.pdfBase64, msg.filename, msg.watermarked, msg.zoom, msg.scrollX, msg.scrollY);
          log('info', 'loadPdf done');
        } catch (err) {
          log('error', 'loadPdf error: ' + err.message);
          showError(err.message);
        }
      } else if (msg.type === 'showError') {
        showError(msg.message);
      }
    });

    // Signal the extension host that the message listener is registered and ready.
    log('info', 'webview sending ready');
    vscodeApi.postMessage({ type: 'ready' });

    function updateZoomSelect(pct) {
      const sel = document.getElementById('zoom-select');
      let custom = sel.querySelector('[data-custom]');
      if (!custom) {
        custom = document.createElement('option');
        custom.setAttribute('data-custom', '');
        sel.appendChild(custom);
      }
      custom.value = 'custom';
      custom.textContent = pct;
      sel.value = 'custom';
    }

    function updateLabel() {
      updateZoomSelect(Math.round(currentScale * 100) + '%');
    }

    const STEP = 1.25;
    document.getElementById('btn-zoom-in').addEventListener('click', async () => {
      currentScale = Math.min(currentScale * STEP, fitScale * 4);
      updateLabel();
      if (pdfDoc) { await renderAll(currentScale); }
    });
    document.getElementById('btn-zoom-out').addEventListener('click', async () => {
      currentScale = Math.max(currentScale / STEP, fitScale / 4);
      updateLabel();
      if (pdfDoc) { await renderAll(currentScale); }
    });
    document.getElementById('zoom-select').addEventListener('change', async (e) => {
      const sel = e.target;
      const val = sel.value;
      // Remove custom option once a named preset is selected.
      const custom = sel.querySelector('[data-custom]');
      if (custom) { sel.removeChild(custom); }
      if (val === 'fit') {
        currentScale = fitScale;
      } else if (val === 'fill') {
        currentScale = fillScale;
      } else if (val === '100pct') {
        currentScale = 1;
      }
      if (pdfDoc) { await renderAll(currentScale); }
    });
    document.getElementById('btn-download').addEventListener('click', () => {
      vscodeApi.postMessage({ type: 'download' });
    });

    function parsePdfDate(raw) {
      if (!raw) { return ''; }
      // D:YYYYMMDDHHmmSS…
      const m = /^D:(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/.exec(raw);
      if (!m) { return raw; }
      const [, yr, mo, dy, hh, mm, ss] = m;
      return \`\${yr}-\${mo}-\${dy} \${hh}:\${mm}:\${ss}\`;
    }

    document.getElementById('btn-props').addEventListener('click', async () => {
      if (!pdfDoc) { return; }
      const meta = await pdfDoc.getMetadata();
      const info = meta.info || {};

      // Page size from first page (PDF user units = 1/72 inch).
      const firstPage = await pdfDoc.getPage(1);
      const vp = firstPage.getViewport({ scale: 1 });
      const wIn = (vp.width  / 72).toFixed(2);
      const hIn = (vp.height / 72).toFixed(2);
      const pageSize = \`\${wIn} \u00d7 \${hIn} in\`;

      const fileSizeStr = (totalBytes / 1024).toFixed(2) + ' KB';

      const required = [
        ['File name',   filename],
        ['File size',   fileSizeStr],
        ['Page size',   pageSize],
        ['Page count',  String(pdfDoc.numPages)],
        ['Title',       info.Title       || ''],
        ['Author',      info.Author      || ''],
        ['Subject',     info.Subject     || ''],
        ['Created',     parsePdfDate(info.CreationDate)],
        ['Modified',    parsePdfDate(info.ModDate)],
        ['Application', info.Creator     || ''],
      ];
      const optional = [
        ['Keywords',    info.Keywords    || ''],
        ['Producer',    info.Producer    || ''],
        ['PDF version', info.PDFFormatVersion || ''],
      ];
      const rows = [...required, ...optional.filter(([, v]) => v !== '')];
      const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
      const table = document.getElementById('props-table');
      table.innerHTML = rows
        .map(([k, v]) => \`<tr><td>\${esc(k)}</td><td>\${esc(v)}</td></tr>\`)
        .join('');
      document.getElementById('props-overlay').classList.add('open');
    });

    function closeProps() { document.getElementById('props-overlay').classList.remove('open'); }
    document.getElementById('btn-props-close').addEventListener('click', closeProps);
    document.getElementById('props-overlay').addEventListener('click', e => {
      if (e.target === e.currentTarget) { closeProps(); }
    });
    document.addEventListener('keydown', e => {
      if (e.key === 'Escape') { closeProps(); }
    });
  </script>
</body>
</html>`;
}
//# sourceMappingURL=preview.js.map