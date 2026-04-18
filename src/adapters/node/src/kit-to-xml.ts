/**
 * kitToXml — serialise an LpdfDocument tree (produced by LpdfKit) back to an
 * lpdf XML string that can be passed directly to `LpdfEngine.renderPdf()` or
 * saved to disk.
 *
 * The serialisation is performed by the Rust core (exposed as a WASM export)
 * so every adapter produces identical XML and any schema change only requires
 * a single Rust update.
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

import type { LpdfDocument } from './kit';

// Load the standalone kit_to_xml export from the compiled WASM module.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { kit_to_xml: wasmKitToXml } = require('../../../../dist/node/lpdf.js') as {
  kit_to_xml: (json: string) => string;
};

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
  return wasmKitToXml(JSON.stringify(doc));
}

