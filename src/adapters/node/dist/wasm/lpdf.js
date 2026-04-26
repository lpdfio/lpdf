/* @ts-self-types="./lpdf.d.ts" */

class LpdfEngine {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        LpdfEngineFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_lpdfengine_free(ptr, 0);
    }
    /**
     * Remove any previously configured encryption.
     */
    clear_encryption() {
        wasm.lpdfengine_clear_encryption(this.__wbg_ptr);
    }
    /**
     * Register raw font bytes (TTF/OTF) for a custom font name.
     * Call this once per font before calling `render_pdf`.
     * Glyph advance-width metrics are extracted automatically from the font
     * bytes so the layout engine can measure text accurately — no separate
     * `set_font_metrics` call is required.
     * @param {string} name
     * @param {Uint8Array} bytes
     */
    load_font(name, bytes) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        wasm.lpdfengine_load_font(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    }
    /**
     * Register raw image bytes (JPEG or PNG) for an image name.
     * Call this for every image referenced by `<img name="…">` nodes.
     * @param {string} name
     * @param {Uint8Array} bytes
     */
    load_image(name, bytes) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        wasm.lpdfengine_load_image(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    }
    /**
     * @param {string} license_key
     */
    constructor(license_key) {
        const ptr0 = passStringToWasm0(license_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.lpdfengine_new(ptr0, len0);
        this.__wbg_ptr = ret >>> 0;
        LpdfEngineFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @param {string} xml
     * @returns {string}
     */
    render(xml) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(xml, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.lpdfengine_render(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Render `xml` to binary PDF bytes.
     *
     * Any custom fonts referenced in `<font src="…">` declarations must have
     * their bytes registered via `load_font` before calling this method.
     *
     * `json_data` is an optional JSON string used to resolve `data-*`
     * attributes in the template.  Pass `None` (or `null` / `undefined` from
     * JavaScript) to render the template with its inline fallback content.
     * @param {string} xml
     * @param {string | null} [json_data]
     * @returns {Uint8Array}
     */
    render_pdf(xml, json_data) {
        const ptr0 = passStringToWasm0(xml, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(json_data) ? 0 : passStringToWasm0(json_data, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.lpdfengine_render_pdf(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * @param {string} json
     * @returns {string}
     */
    render_tree(json) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.lpdfengine_render_tree(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Render a JSON kit-tree or canvas-tree document to PDF bytes.
     *
     * This is the JSON counterpart of `render_pdf`. The Node adapter uses it
     * when an `LpdfDocument` Kit tree is passed to `renderPdf()`, avoiding an
     * intermediate XML serialisation step. PHP, Python, and .NET adapters also
     * use this entry point.
     * @param {string} json
     * @returns {Uint8Array}
     */
    render_tree_pdf(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.lpdfengine_render_tree_pdf(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * Set an optional ISO 8601 creation timestamp (e.g. `"2024-06-01T12:00:00"`).
     * When provided, written as `/CreationDate` in the PDF info dictionary.
     * Omitting this keeps builds reproducible (no embedded timestamp).
     * @param {string} iso
     */
    set_created_on(iso) {
        const ptr0 = passStringToWasm0(iso, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.lpdfengine_set_created_on(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Configure RC4-128 encryption applied to every subsequent `render_pdf` call.
     *
     * `permissions_json` is a JSON object with boolean fields matching the
     * `Permissions` struct (`print`, `modify`, `copy`, `annotate`, `fill_forms`,
     * `accessibility`, `assemble`, `print_hq`). Omitted fields default to `true`.
     *
     * To apply permissions without an open password, pass an empty `user_password`
     * and a non-empty `owner_password`.
     * @param {string} user_password
     * @param {string} owner_password
     * @param {string} permissions_json
     */
    set_encryption(user_password, owner_password, permissions_json) {
        const ptr0 = passStringToWasm0(user_password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(owner_password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(permissions_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        wasm.lpdfengine_set_encryption(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
    }
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
     * @param {string} json
     */
    set_font_metrics(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.lpdfengine_set_font_metrics(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Set the current Unix timestamp (seconds) for license expiry checking.
     * Must be called before `render_pdf` when using a time-limited token.
     * If not set (default `0`), expiry is not checked.
     * @param {bigint} unix
     */
    set_now(unix) {
        wasm.lpdfengine_set_now(this.__wbg_ptr, unix);
    }
}
if (Symbol.dispose) LpdfEngine.prototype[Symbol.dispose] = LpdfEngine.prototype.free;
exports.LpdfEngine = LpdfEngine;

/**
 * Convert a JSON kit-tree (produced by `LpdfKit` in any adapter) to an lpdf
 * XML string.
 *
 * Useful for debugging Kit-generated documents, saving them as `.xml` files,
 * or feeding them into the XML render path. The output is equivalent to
 * hand-authored XML and passes through `render_pdf` without modification.
 * @param {string} json
 * @returns {string}
 */
function kit_to_xml(json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.kit_to_xml(ptr0, len0);
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}
exports.kit_to_xml = kit_to_xml;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_6b64449b9b9ed33c: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./lpdf_bg.js": import0,
    };
}

const LpdfEngineFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_lpdfengine_free(ptr >>> 0, 1));

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
function decodeText(ptr, len) {
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

const wasmPath = `${__dirname}/lpdf_bg.wasm`;
const wasmBytes = require('fs').readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBytes);
let wasm = new WebAssembly.Instance(wasmModule, __wbg_get_imports()).exports;
wasm.__wbindgen_start();
