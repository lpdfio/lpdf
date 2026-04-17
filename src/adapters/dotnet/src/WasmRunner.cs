using System.Buffers.Binary;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Wasmtime;
using WasmModule = Wasmtime.Module;

namespace Lpdf;

/// <summary>
/// Thin wrapper around the Wasmtime.NET engine that loads the embedded
/// <c>lpdf.wasm</c> WASI binary and exposes <c>render</c>/<c>render_tree</c>
/// as synchronous string-in / string-out calls, and <c>render_pdf</c> /
/// <c>render_tree_pdf</c> as calls that return raw PDF bytes.
/// </summary>
internal sealed class WasmRunner : IDisposable
{
    // The engine and module are expensive to create and thread-safe — share them.
    private static readonly Engine      _engine;
    private static readonly WasmModule  _module;
    private static readonly Linker      _linker;

    static WasmRunner()
    {
        _engine = new Engine();
        _module = WasmModule.FromBytes(_engine, "lpdf", LoadWasmBytes());
        _linker = new Linker(_engine);
        _linker.DefineWasi();
    }

    private bool _disposed;

    // ── Public ────────────────────────────────────────────────────────────────

    /// <summary>Render an lpdf XML document and return the RenderTree JSON string.</summary>
    public string Render(string xml, string licenseKey)
        => Invoke("render", xml, licenseKey);

    /// <summary>Render an lpdf JSON tree and return the RenderTree JSON string.</summary>
    public string RenderTree(string json, string licenseKey)
        => Invoke("render_tree", json, licenseKey);

    /// <summary>
    /// Render an lpdf XML document to raw PDF bytes.
    /// Custom fonts are base64-encoded into the request envelope.
    /// If <paramref name="srcFallback"/> is provided, a preliminary <c>render</c>
    /// call is made to discover font <c>src</c> paths so they can be resolved
    /// before the PDF render call.
    /// </summary>
    public byte[] RenderPdf(
        string xml,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        Func<string, byte[]>?                srcFallback)
        => InvokeRenderPdf("render_pdf", xml, licenseKey, fontBytes, srcFallback, isTree: false);

    /// <summary>
    /// Render an lpdf JSON document tree to raw PDF bytes.
    /// </summary>
    public byte[] RenderTreePdf(
        string json,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        Func<string, byte[]>?                srcFallback)
        => InvokeRenderPdf("render_tree_pdf", json, licenseKey, fontBytes, srcFallback, isTree: true);

    // ── Internals ─────────────────────────────────────────────────────────────

    private byte[] InvokeRenderPdf(
        string method,
        string input,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        Func<string, byte[]>?                srcFallback,
        bool                                 isTree)
    {
        var fonts = ResolveAllFonts(input, licenseKey, fontBytes, srcFallback, isTree);

        // Build the request object, including font bytes as base64.
        var fontsNode = new JsonObject();
        foreach (var (name, bytes) in fonts)
            fontsNode[name] = Convert.ToBase64String(bytes);

        // Extract glyph advance widths from each font binary and pass to the WASI
        // engine so the Rust layout pass measures custom-font text accurately.
        var metricsNode = new JsonObject();
        foreach (var (name, bytes) in fonts)
        {
            var widths = ExtractFontWidths(bytes);
            if (widths is not null)
                metricsNode[name] = widths;
        }

        var requestObj = new JsonObject
        {
            ["method"]  = method,
            ["key"]     = licenseKey,
            ["input"]   = input,
            ["fonts"]   = fontsNode,
            ["metrics"] = metricsNode,
        };
        var requestBytes = Encoding.UTF8.GetBytes(requestObj.ToJsonString());

        var responseJson = RunWasm(requestBytes);

        using var doc = JsonDocument.Parse(responseJson);
        var root = doc.RootElement;

        if (root.TryGetProperty("error", out var errEl))
            throw new LpdfRenderException(errEl.GetString() ?? "Unknown render error.");

        if (!root.TryGetProperty("pdf", out var pdfEl))
            throw new InvalidOperationException("WASM render_pdf response missing 'pdf' field.");

        return Convert.FromBase64String(pdfEl.GetString()
            ?? throw new InvalidOperationException("WASM render_pdf 'pdf' field is null."));
    }

    /// <summary>
    /// When <paramref name="srcFallback"/> is set, performs a preliminary
    /// <c>render</c> call to discover font <c>src</c> paths, then resolves
    /// them. Returns the merged font bytes dictionary.
    /// </summary>
    private IReadOnlyDictionary<string, byte[]> ResolveAllFonts(
        string input,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        Func<string, byte[]>?                srcFallback,
        bool                                 isTree)
    {
        if (srcFallback is null)
            return fontBytes ?? new Dictionary<string, byte[]>();

        // Discover which fonts have src paths by doing a lightweight render call.
        var renderJson = isTree ? RenderTree(input, licenseKey) : Render(input, licenseKey);
        using var doc = JsonDocument.Parse(renderJson);

        var merged = new Dictionary<string, byte[]>(fontBytes ?? new Dictionary<string, byte[]>());

        if (doc.RootElement.TryGetProperty("meta", out var meta) &&
            meta.TryGetProperty("fonts", out var fontsEl))
        {
            foreach (var font in fontsEl.EnumerateObject())
            {
                var name = font.Name;
                if (merged.ContainsKey(name)) continue;  // FontBytes takes priority
                if (font.Value.TryGetProperty("src", out var srcEl))
                {
                    var src = srcEl.GetString();
                    if (src is not null)
                    {
                        try   { merged[name] = srcFallback(src); }
                        catch { /* skip unresolvable fonts — WASM will fall back or error */ }
                    }
                }
            }
        }

        return merged;
    }

    private static string Invoke(string method, string input, string key)
    {
        var request = JsonSerializer.Serialize(new { method, key, input });
        return RunWasm(Encoding.UTF8.GetBytes(request));
    }

    private static string RunWasm(byte[] requestBytes)
    {
        var inputPath  = Path.GetTempFileName();
        var outputPath = Path.GetTempFileName();
        try
        {
            File.WriteAllBytes(inputPath, requestBytes);

            var wasiConfig = new WasiConfiguration()
                .WithStandardInput(inputPath)
                .WithStandardOutput(outputPath);

            using (var store = new Store(_engine))
            {
                store.SetWasiConfiguration(wasiConfig);
                var instance = _linker.Instantiate(store, _module);
                var start = instance.GetAction("_start")
                    ?? throw new InvalidOperationException("lpdf WASM module does not export '_start'.");
                start();
            } // store disposed here → WASI file handles released before we read

            return File.ReadAllText(outputPath, Encoding.UTF8);
        }
        finally
        {
            try { File.Delete(inputPath); }  catch { /* best-effort cleanup */ }
            try { File.Delete(outputPath); } catch { /* best-effort cleanup */ }
        }
    }

    /// <summary>
    /// Parse the head/hhea/cmap/hmtx tables from a TrueType or OpenType font binary
    /// and return per-glyph advance widths for printable ASCII (code points 32–126),
    /// normalised to 1/1000 em units — the format expected by the Rust layout engine.
    /// Returns <c>null</c> if the font cannot be parsed (WOFF/WOFF2/unsupported cmap).
    /// </summary>
    private static JsonObject? ExtractFontWidths(byte[] data)
    {
        if (data.Length < 12) return null;

        ushort U16(int off) => BinaryPrimitives.ReadUInt16BigEndian(data.AsSpan(off));
        uint   U32(int off) => BinaryPrimitives.ReadUInt32BigEndian(data.AsSpan(off));
        short  I16(int off) => BinaryPrimitives.ReadInt16BigEndian(data.AsSpan(off));

        // ── sfnt table directory ──────────────────────────────────────────────
        int numTables = U16(4);
        var tables = new Dictionary<string, int>();
        for (int i = 0; i < numTables; i++)
        {
            int b = 12 + i * 16;
            if (b + 16 > data.Length) return null;
            var tag = Encoding.ASCII.GetString(data, b, 4);
            tables[tag] = (int)U32(b + 8);
        }

        if (!tables.ContainsKey("head") || !tables.ContainsKey("cmap") ||
            !tables.ContainsKey("hmtx") || !tables.ContainsKey("hhea"))
            return null;

        // ── units-per-em (head, offset 18) ───────────────────────────────────
        int upm = U16(tables["head"] + 18);
        if (upm == 0) return null;

        // ── numOfLongHorMetrics (hhea, offset 34) ────────────────────────────
        int numHMetrics = U16(tables["hhea"] + 34);
        if (numHMetrics == 0) return null;

        // ── glyph advance width from hmtx ────────────────────────────────────
        int hmtxBase = tables["hmtx"];
        int GetAdvance(int glyphId)
        {
            int idx = Math.Min(glyphId, numHMetrics - 1);
            return U16(hmtxBase + idx * 4);
        }

        // ── find Unicode BMP cmap subtable ────────────────────────────────────
        int cmapBase   = tables["cmap"];
        int numEncTbls = U16(cmapBase + 2);
        int subtableOff  = -1;
        int bestPriority = 999;
        for (int i = 0; i < numEncTbls; i++)
        {
            int b          = cmapBase + 4 + i * 8;
            int platformId = U16(b);
            int encodingId = U16(b + 2);
            int off        = cmapBase + (int)U32(b + 4);
            if (platformId == 3 && encodingId == 1 && bestPriority > 0)
            { subtableOff = off; bestPriority = 0; }
            else if (platformId == 0 && bestPriority > 1)
            { subtableOff = off; bestPriority = 1; }
        }
        if (subtableOff < 0) return null;

        // ── parse cmap format 4 ───────────────────────────────────────────────
        if (U16(subtableOff) != 4) return null;

        int segCount      = U16(subtableOff + 6) >> 1;
        int endCodesOff   = subtableOff + 14;
        int startCodesOff = endCodesOff   + segCount * 2 + 2; // +2 for reservedPad
        int idDeltaOff    = startCodesOff + segCount * 2;
        int idRangeOff    = idDeltaOff    + segCount * 2;

        int GetGlyphId(int cp)
        {
            for (int s = 0; s < segCount; s++)
            {
                int end = U16(endCodesOff + s * 2);
                if (cp > end) continue;
                int start    = U16(startCodesOff + s * 2);
                if (cp < start) return 0;
                int delta    = I16(idDeltaOff + s * 2);
                int rangeOff = U16(idRangeOff + s * 2);
                if (rangeOff == 0)
                    return (cp + delta) & 0xFFFF;
                int glyphOff = idRangeOff + s * 2 + rangeOff + (cp - start) * 2;
                int glyphId  = U16(glyphOff);
                return glyphId == 0 ? 0 : (glyphId + delta) & 0xFFFF;
            }
            return 0;
        }

        // ── sample ASCII range (32–126) ───────────────────────────────────────
        var ascii = new JsonArray();
        int sum = 0, count = 0;
        for (int cp = 32; cp <= 126; cp++)
        {
            int glyphId = GetGlyphId(cp);
            int adv     = glyphId > 0 ? GetAdvance(glyphId) : GetAdvance(0);
            int w       = (int)Math.Round(adv * 1000.0 / upm);
            ascii.Add(w);
            if (w > 0) { sum += w; count++; }
        }

        int def = count > 0 ? (int)Math.Round((double)sum / count) : 500;
        return new JsonObject { ["default"] = def, ["ascii"] = ascii };
    }

    private static byte[] LoadWasmBytes()
    {
        var asm  = Assembly.GetExecutingAssembly();
        const string resourceName = "Lpdf.wasm.lpdf.wasm";
        using var stream = asm.GetManifestResourceStream(resourceName)
            ?? throw new InvalidOperationException(
                $"Embedded WASM resource '{resourceName}' not found. " +
                "Run 'make wasi' first to build dist/wasi/lpdf.wasm, " +
                "then copy it to src/adapters/dotnet/wasm/lpdf.wasm.");

        using var ms = new MemoryStream((int)stream.Length);
        stream.CopyTo(ms);
        return ms.ToArray();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        // _engine, _module, _linker are shared — not disposed here.
    }
}

