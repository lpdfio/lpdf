// ../../../dist/web/lpdf.js
var LpdfEngine = class {
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
};
if (Symbol.dispose) LpdfEngine.prototype[Symbol.dispose] = LpdfEngine.prototype.free;
function __wbg_get_imports() {
  const import0 = {
    __proto__: null,
    __wbg___wbindgen_throw_6b64449b9b9ed33c: function(arg0, arg1) {
      throw new Error(getStringFromWasm0(arg0, arg1));
    },
    __wbindgen_cast_0000000000000001: function(arg0, arg1) {
      const ret = getStringFromWasm0(arg0, arg1);
      return ret;
    },
    __wbindgen_init_externref_table: function() {
      const table = wasm.__wbindgen_externrefs;
      const offset = table.grow(4);
      table.set(0, void 0);
      table.set(offset + 0, void 0);
      table.set(offset + 1, null);
      table.set(offset + 2, true);
      table.set(offset + 3, false);
    }
  };
  return {
    __proto__: null,
    "./lpdf_bg.js": import0
  };
}
var LpdfEngineFinalization = typeof FinalizationRegistry === "undefined" ? { register: () => {
}, unregister: () => {
} } : new FinalizationRegistry((ptr) => wasm.__wbg_lpdfengine_free(ptr >>> 0, 1));
function getArrayU8FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}
function getStringFromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return decodeText(ptr, len);
}
var cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
  if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
    cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
  }
  return cachedUint8ArrayMemory0;
}
function isLikeNone(x) {
  return x === void 0 || x === null;
}
function passArray8ToWasm0(arg, malloc) {
  const ptr = malloc(arg.length * 1, 1) >>> 0;
  getUint8ArrayMemory0().set(arg, ptr / 1);
  WASM_VECTOR_LEN = arg.length;
  return ptr;
}
function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === void 0) {
    const buf = cachedTextEncoder.encode(arg);
    const ptr2 = malloc(buf.length, 1) >>> 0;
    getUint8ArrayMemory0().subarray(ptr2, ptr2 + buf.length).set(buf);
    WASM_VECTOR_LEN = buf.length;
    return ptr2;
  }
  let len = arg.length;
  let ptr = malloc(len, 1) >>> 0;
  const mem = getUint8ArrayMemory0();
  let offset = 0;
  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset);
    if (code > 127) break;
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
var cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
var MAX_SAFARI_DECODE_BYTES = 2146435072;
var numBytesDecoded = 0;
function decodeText(ptr, len) {
  numBytesDecoded += len;
  if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
    cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
    cachedTextDecoder.decode();
    numBytesDecoded = len;
  }
  return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}
var cachedTextEncoder = new TextEncoder();
if (!("encodeInto" in cachedTextEncoder)) {
  cachedTextEncoder.encodeInto = function(arg, view) {
    const buf = cachedTextEncoder.encode(arg);
    view.set(buf);
    return {
      read: arg.length,
      written: buf.length
    };
  };
}
var WASM_VECTOR_LEN = 0;
var wasmModule;
var wasm;
function __wbg_finalize_init(instance, module) {
  wasm = instance.exports;
  wasmModule = module;
  cachedUint8ArrayMemory0 = null;
  wasm.__wbindgen_start();
  return wasm;
}
async function __wbg_load(module, imports) {
  if (typeof Response === "function" && module instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming === "function") {
      try {
        return await WebAssembly.instantiateStreaming(module, imports);
      } catch (e) {
        const validResponse = module.ok && expectedResponseType(module.type);
        if (validResponse && module.headers.get("Content-Type") !== "application/wasm") {
          console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);
        } else {
          throw e;
        }
      }
    }
    const bytes = await module.arrayBuffer();
    return await WebAssembly.instantiate(bytes, imports);
  } else {
    const instance = await WebAssembly.instantiate(module, imports);
    if (instance instanceof WebAssembly.Instance) {
      return { instance, module };
    } else {
      return instance;
    }
  }
  function expectedResponseType(type) {
    switch (type) {
      case "basic":
      case "cors":
      case "default":
        return true;
    }
    return false;
  }
}
async function __wbg_init(module_or_path) {
  if (wasm !== void 0) return wasm;
  if (module_or_path !== void 0) {
    if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
      ({ module_or_path } = module_or_path);
    } else {
      console.warn("using deprecated parameters for the initialization function; pass a single object instead");
    }
  }
  if (module_or_path === void 0) {
    module_or_path = new URL("lpdf_bg.wasm", import.meta.url);
  }
  const imports = __wbg_get_imports();
  if (typeof module_or_path === "string" || typeof Request === "function" && module_or_path instanceof Request || typeof URL === "function" && module_or_path instanceof URL) {
    module_or_path = fetch(module_or_path);
  }
  const { instance, module } = await __wbg_load(await module_or_path, imports);
  return __wbg_finalize_init(instance, module);
}

// src/kit.ts
function attrKey(camel) {
  return camel.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
}
function buildAttrs(options) {
  const result = {};
  for (const [key, val] of Object.entries(options)) {
    if (val !== void 0) {
      result[attrKey(key)] = val;
    }
  }
  return result;
}
function layout(nodes = []) {
  return { type: "layout", nodes };
}
function canvas(layers = []) {
  return { type: "canvas", nodes: layers };
}
function section(input = {}) {
  return {
    type: "section",
    attrs: buildAttrs(input.options ?? {}),
    nodes: input.nodes ?? []
  };
}
function document(input = {}) {
  const { tokens, meta, ...restOpts } = input.options ?? {};
  const attrs = {
    ...buildAttrs(restOpts)
  };
  if (tokens !== void 0) attrs["tokens"] = tokens;
  if (meta !== void 0) attrs["meta"] = meta;
  return {
    version: 1,
    type: "document",
    attrs,
    nodes: input.sections ?? []
  };
}
var LpdfKit = Object.freeze({
  layout,
  canvas,
  section,
  document
});

// src/browser.ts
async function initLpdf(wasmSource, licenseKey = "", initOptions = {}) {
  await __wbg_init(wasmSource);
  const fontMap = /* @__PURE__ */ new Map();
  const imageMap = /* @__PURE__ */ new Map();
  if (initOptions.fontBytes) {
    for (const [name, bytes] of Object.entries(initOptions.fontBytes)) {
      fontMap.set(name, bytes);
    }
  }
  return {
    loadFont(name, bytes) {
      fontMap.set(name, bytes);
    },
    loadImage(name, bytes) {
      imageMap.set(name, bytes);
    },
    async renderPdf(xml, callOptions = {}) {
      const engine = new LpdfEngine(licenseKey);
      for (const [name, bytes] of fontMap) {
        engine.load_font(name, bytes);
      }
      if (callOptions.fontBytes) {
        for (const [name, bytes] of Object.entries(callOptions.fontBytes)) {
          if (!fontMap.has(name)) engine.load_font(name, bytes);
        }
      }
      for (const [name, bytes] of imageMap) {
        engine.load_image(name, bytes);
      }
      const dataJson = callOptions.data != null ? JSON.stringify(callOptions.data) : null;
      const pdf = engine.render_pdf(xml, dataJson);
      engine.free();
      return pdf;
    }
  };
}
export {
  LpdfKit,
  initLpdf
};
