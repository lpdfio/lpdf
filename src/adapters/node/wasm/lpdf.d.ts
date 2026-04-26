/* tslint:disable */
/* eslint-disable */

export class LpdfEngine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Remove any previously configured encryption.
     */
    clear_encryption(): void;
    /**
     * Register raw font bytes (TTF/OTF) for a custom font name.
     * Call this once per font before calling `render_pdf`.
     * Glyph advance-width metrics are extracted automatically from the font
     * bytes so the layout engine can measure text accurately — no separate
     * `set_font_metrics` call is required.
     */
    load_font(name: string, bytes: Uint8Array): void;
    /**
     * Register raw image bytes (JPEG or PNG) for an image name.
     * Call this for every image referenced by `<img name="…">` nodes.
     */
    load_image(name: string, bytes: Uint8Array): void;
    constructor(license_key: string);
    render(xml: string): string;
    /**
     * Render `xml` to binary PDF bytes.
     *
     * Any custom fonts referenced in `<font src="…">` declarations must have
     * their bytes registered via `load_font` before calling this method.
     *
     * `json_data` is an optional JSON string used to resolve `data-*`
     * attributes in the template.  Pass `None` (or `null` / `undefined` from
     * JavaScript) to render the template with its inline fallback content.
     */
    render_pdf(xml: string, json_data?: string | null): Uint8Array;
    render_tree(json: string): string;
    /**
     * Render a JSON kit-tree or canvas-tree document to PDF bytes.
     *
     * This is the JSON counterpart of `render_pdf`. The Node adapter uses it
     * when an `LpdfDocument` Kit tree is passed to `renderPdf()`, avoiding an
     * intermediate XML serialisation step. PHP, Python, and .NET adapters also
     * use this entry point.
     */
    render_tree_pdf(json: string): Uint8Array;
    /**
     * Set an optional ISO 8601 creation timestamp (e.g. `"2024-06-01T12:00:00"`).
     * When provided, written as `/CreationDate` in the PDF info dictionary.
     * Omitting this keeps builds reproducible (no embedded timestamp).
     */
    set_created_on(iso: string): void;
    /**
     * Configure RC4-128 encryption applied to every subsequent `render_pdf` call.
     *
     * `permissions_json` is a JSON object with boolean fields matching the
     * `Permissions` struct (`print`, `modify`, `copy`, `annotate`, `fill_forms`,
     * `accessibility`, `assemble`, `print_hq`). Omitted fields default to `true`.
     *
     * To apply permissions without an open password, pass an empty `user_password`
     * and a non-empty `owner_password`.
     */
    set_encryption(user_password: string, owner_password: string, permissions_json: string): void;
    /**
     * Inject glyph advance-width tables for custom fonts.
     *
     * Call this *before* `render_pdf` / `render` when the document uses custom
     * fonts (declared via `<font src="…"`). The adapter extracts these widths
     * from the font binary and passes them as a JSON object:
     *
     * ```json
     * { "fontName": { "default": 500, "ascii": [260, 285, …] } }
     * ```
     *
     * `ascii` is a 95-element array for code points 32–126. `default` is used
     * for code points outside that range. All values are in 1/1000 em units.
     */
    set_font_metrics(json: string): void;
    /**
     * Set the current Unix timestamp (seconds) for license expiry checking.
     * Must be called before `render_pdf` when using a time-limited token.
     * If not set (default `0`), expiry is not checked.
     */
    set_now(unix: bigint): void;
}

/**
 * Convert a JSON kit-tree (produced by `LpdfKit` in any adapter) to an lpdf
 * XML string.
 *
 * Useful for debugging Kit-generated documents, saving them as `.xml` files,
 * or feeding them into the XML render path. The output is equivalent to
 * hand-authored XML and passes through `render_pdf` without modification.
 */
export function kit_to_xml(json: string): string;
