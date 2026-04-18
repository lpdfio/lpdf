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
}
