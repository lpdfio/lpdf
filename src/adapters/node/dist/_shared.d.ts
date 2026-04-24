/**
 * Shared types used by both the Node.js and browser entry points.
 */
export interface RenderOptions {
    /**
     * Pre-loaded font bytes for custom fonts referenced via <fonts src="…">.
     * Keys are the font names used in the document; values are raw TTF/OTF bytes.
     * @deprecated Pass font bytes via `LpdfEngine.loadFont()` instead.
     */
    fontBytes?: Record<string, Uint8Array>;
    /**
     * Optional ISO 8601 creation timestamp (e.g. `"2024-06-01T12:00:00"`).
     * When provided, written as `/CreationDate` in the PDF info dictionary.
     * Omitting this keeps builds reproducible (no embedded timestamp).
     */
    createdOn?: string;
    /**
     * Optional data object for resolving `data-*` binding attributes in the
     * XML template.  Pass `null` or omit to render with inline fallback content.
     * Only applies when `input` is an XML string.
     */
    data?: Record<string, unknown> | null;
}
/**
 * Page scope for canvas layers and layout regions.
 * Numeric ranges (e.g. '2-4', '1,3-5', '2-last') remain as plain strings.
 */
export type PageScope = 'each' | 'first' | 'last' | 'odd' | 'even';
