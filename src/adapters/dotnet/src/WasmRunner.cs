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

        var requestObj = new JsonObject
        {
            ["method"]  = method,
            ["key"]     = licenseKey,
            ["input"]   = input,
            ["fonts"]   = fontsNode,
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

