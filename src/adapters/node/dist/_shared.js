"use strict";
/**
 * Shared render-tree types used by both the Node.js and browser entry points.
 * PDF generation now happens inside the Rust core (pdf.rs) and is surfaced via
 * the WASM `render_pdf()` method — no pdf-lib dependency required.
 */
Object.defineProperty(exports, "__esModule", { value: true });
