/**
 * Type shim for the wasm-pack web build.
 * Used by src/browser.ts so the import is typed without needing to reference
 * the actual generated dist file from TypeScript.
 */
declare module '../../../../dist/web/lpdf.js' {
  export class LpdfEngine {
    constructor(license_key: string): LpdfEngine;
    render(xml: string): string;
    free(): void;
    [Symbol.dispose](): void;
  }
  export function initSync(module: unknown): void;
  export default function init(
    source?: string | URL | Response | BufferSource | null,
  ): Promise<void>;
}
