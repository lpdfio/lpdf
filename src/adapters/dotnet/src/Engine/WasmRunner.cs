using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using Wasmtime;
using WasmEngine = Wasmtime.Engine;
using WasmModule = Wasmtime.Module;

namespace Lpdf.Engine;

/// <summary>
/// Thin wrapper around the Wasmtime.NET engine that loads the embedded
/// <c>lpdf.wasm</c> WASI binary and exposes <c>render</c>/<c>render_tree</c>
/// as synchronous string-in / string-out calls, and <c>render_pdf</c> /
/// <c>render_tree_pdf</c> as calls that return raw PDF bytes.
/// </summary>
internal sealed class WasmRunner : IDisposable
{
    // The engine and module are expensive to create and thread-safe — share them.
    private static readonly WasmEngine   _engine;
    private static readonly WasmModule  _module;
    private static readonly Linker      _linker;

    static WasmRunner()
    {
        _engine = new WasmEngine();
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
        IReadOnlyDictionary<string, byte[]>? imageBytes,
        Func<string, byte[]>?                srcFallback,
        string?                              encryptJson = null,
        string?                              createdOn = null,
        string?                              dataJson = null)
        => InvokeRenderPdf("render_pdf", xml, licenseKey, fontBytes, imageBytes, srcFallback, isTree: false, encryptJson, createdOn, dataJson);

    /// <summary>
    /// Render an lpdf JSON document tree to raw PDF bytes.
    /// </summary>
    public byte[] RenderTreePdf(
        string json,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        IReadOnlyDictionary<string, byte[]>? imageBytes,
        Func<string, byte[]>?                srcFallback,
        string?                              encryptJson = null,
        string?                              createdOn = null)
        => InvokeRenderPdf("render_tree_pdf", json, licenseKey, fontBytes, imageBytes, srcFallback, isTree: true, encryptJson, createdOn);

    /// <summary>
    /// Convert a JSON kit-tree (produced by <c>LpdfKit</c>) to an lpdf XML string.
    /// </summary>
    public string KitToXml(string json)
    {
        var requestObj  = new JsonObject { ["method"] = "kit_to_xml", ["input"] = json };
        var requestBytes = Encoding.UTF8.GetBytes(requestObj.ToJsonString());
        var responseJson = RunWasm(requestBytes);

        using var doc = JsonDocument.Parse(responseJson);
        var root = doc.RootElement;

        if (root.TryGetProperty("error", out var errEl))
            throw new EngineException(errEl.GetString() ?? "Unknown kit_to_xml error.");

        if (!root.TryGetProperty("xml", out var xmlEl))
            throw new InvalidOperationException("WASM kit_to_xml response missing 'xml' field.");

        return xmlEl.GetString()
            ?? throw new InvalidOperationException("WASM kit_to_xml 'xml' field is null.");
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    private byte[] InvokeRenderPdf(
        string method,
        string input,
        string licenseKey,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        IReadOnlyDictionary<string, byte[]>? imageBytes,
        Func<string, byte[]>?                srcFallback,
        bool                                 isTree,
        string?                              encryptJson = null,
        string?                              createdOn = null,
        string?                              dataJson = null)
    {
        var fonts  = ResolveAllFonts(input, fontBytes, srcFallback, isTree);
        var images = ResolveAllImages(input, imageBytes, srcFallback, isTree);

        var fontsNode = new JsonObject();
        foreach (var (name, bytes) in fonts)
            fontsNode[name] = Convert.ToBase64String(bytes);

        var requestObj = new JsonObject
        {
            ["method"]  = method,
            ["key"]     = licenseKey,
            ["input"]   = input,
            ["fonts"]   = fontsNode,
        };

        if (images is { Count: > 0 })
        {
            var imagesNode = new JsonObject();
            foreach (var (name, bytes) in images)
                imagesNode[name] = Convert.ToBase64String(bytes);
            requestObj["images"] = imagesNode;
        }

        if (encryptJson is not null)
            requestObj["encrypt"] = JsonNode.Parse(encryptJson);

        if (createdOn is not null)
            requestObj["created_on"] = createdOn;

        if (dataJson is not null)
            requestObj["data"] = JsonNode.Parse(dataJson);

        var requestBytes = Encoding.UTF8.GetBytes(requestObj.ToJsonString());
        var responseJson = RunWasm(requestBytes);

        using var doc = JsonDocument.Parse(responseJson);
        var root = doc.RootElement;

        if (root.TryGetProperty("error", out var errEl))
            throw new EngineException(errEl.GetString() ?? "Unknown render error.");

        if (!root.TryGetProperty("pdf", out var pdfEl))
            throw new InvalidOperationException("WASM render_pdf response missing 'pdf' field.");

        return Convert.FromBase64String(pdfEl.GetString()
            ?? throw new InvalidOperationException("WASM render_pdf 'pdf' field is null."));
    }

    private static IReadOnlyDictionary<string, byte[]> ResolveAllFonts(
        string input,
        IReadOnlyDictionary<string, byte[]>? fontBytes,
        Func<string, byte[]>?                srcFallback,
        bool                                 isTree)
    {
        if (srcFallback is null)
            return fontBytes ?? new Dictionary<string, byte[]>();

        var merged = new Dictionary<string, byte[]>(fontBytes ?? new Dictionary<string, byte[]>());
        var srcPaths = isTree ? ExtractFontSrcsFromJson(input) : ExtractFontSrcsFromXml(input);
        foreach (var (name, src) in srcPaths)
        {
            if (merged.ContainsKey(name)) continue;
            try   { merged[name] = srcFallback(src); }
            catch { /* skip unresolvable fonts */ }
        }
        return merged;
    }

    private static IReadOnlyDictionary<string, byte[]> ResolveAllImages(
        string input,
        IReadOnlyDictionary<string, byte[]>? imageBytes,
        Func<string, byte[]>?                srcFallback,
        bool                                 isTree)
    {
        var merged = new Dictionary<string, byte[]>(imageBytes ?? new Dictionary<string, byte[]>());
        if (srcFallback is null) return merged;

        var srcPaths = isTree ? ExtractImageSrcsFromJson(input) : ExtractImageSrcsFromXml(input);
        foreach (var (name, src) in srcPaths)
        {
            if (merged.ContainsKey(name)) continue;
            try   { merged[name] = srcFallback(src); }
            catch { /* skip unresolvable images */ }
        }
        return merged;
    }

    private static Dictionary<string, string> ExtractFontSrcsFromXml(string xml)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (Match m in Regex.Matches(xml, @"<font\s[^>]*>"))
        {
            var tag  = m.Value;
            var name = Regex.Match(tag, @"\bname=""([^""]*)""").Groups[1].Value;
            var refv = Regex.Match(tag, @"\bref=""([^""]*)""").Groups[1].Value;
            var src  = Regex.Match(tag, @"\bsrc=""([^""]*)""").Groups[1].Value;
            var key  = !string.IsNullOrEmpty(refv) ? refv : name;
            if (!string.IsNullOrEmpty(key) && !string.IsNullOrEmpty(src))
                result[key] = src;
        }
        return result;
    }

    private static Dictionary<string, string> ExtractImageSrcsFromXml(string xml)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (Match m in Regex.Matches(xml, @"<image\s[^>]*>"))
        {
            var tag  = m.Value;
            var name = Regex.Match(tag, @"\bname=""([^""]*)""").Groups[1].Value;
            var refv = Regex.Match(tag, @"\bref=""([^""]*)""").Groups[1].Value;
            var src  = Regex.Match(tag, @"\bsrc=""([^""]*)""").Groups[1].Value;
            var key  = !string.IsNullOrEmpty(refv) ? refv : name;
            if (!string.IsNullOrEmpty(key) && !string.IsNullOrEmpty(src))
                result[key] = src;
        }
        return result;
    }

    private static Dictionary<string, string> ExtractFontSrcsFromJson(string json)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;
            if (!root.TryGetProperty("attrs", out var attrs)) return result;
            if (!attrs.TryGetProperty("tokens", out var tokens)) return result;
            if (!tokens.TryGetProperty("fonts", out var fonts)) return result;
            foreach (var font in fonts.EnumerateObject())
            {
                if (font.Value.TryGetProperty("src", out var srcEl))
                {
                    var src = srcEl.GetString();
                    if (src is null) continue;
                    var key = font.Value.TryGetProperty("ref", out var refEl)
                        ? (refEl.GetString() ?? font.Name)
                        : font.Name;
                    result[key] = src;
                }
            }
        }
        catch { /* malformed JSON */ }
        return result;
    }

    private static Dictionary<string, string> ExtractImageSrcsFromJson(string json)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;
            if (!root.TryGetProperty("attrs", out var attrs)) return result;
            if (!attrs.TryGetProperty("tokens", out var tokens)) return result;
            if (!tokens.TryGetProperty("images", out var images)) return result;
            foreach (var img in images.EnumerateObject())
            {
                if (img.Value.TryGetProperty("src", out var srcEl))
                {
                    var src = srcEl.GetString();
                    if (src is null) continue;
                    var key = img.Value.TryGetProperty("ref", out var refEl)
                        ? (refEl.GetString() ?? img.Name)
                        : img.Name;
                    result[key] = src;
                }
            }
        }
        catch { /* malformed JSON */ }
        return result;
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
            }

            return File.ReadAllText(outputPath, Encoding.UTF8);
        }
        finally
        {
            try { File.Delete(inputPath); }  catch { /* best-effort cleanup */ }
            try { File.Delete(outputPath); } catch { /* best-effort cleanup */ }
        }
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
