/**
 * Type shim for the wasm-pack web build.
 * Used by src/browser.ts so the import is typed without needing to reference
 * the actual generated dist file from TypeScript.
 */
declare module '../../../../dist/web/lpdf.js' {
  export class LpdfEngine {
    constructor(license_key: string): LpdfEngine;
    /** Register raw font bytes for a custom font name. */
    load_font(name: string, bytes: Uint8Array): void;
    /** Set an optional ISO 8601 creation timestamp for /CreationDate. */
    set_created_on(iso: string): void;
    /** Render XML to binary PDF bytes. */
    render_pdf(xml: string): Uint8Array;
    free(): void;
    [Symbol.dispose](): void;
  }
  export function initSync(module: unknown): void;
  export default function init(
    source?: string | URL | Response | BufferSource | null,
  ): Promise<void>;
}
