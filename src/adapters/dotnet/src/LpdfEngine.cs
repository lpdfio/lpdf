using System.Text.Json;

namespace Lpdf;

/// <summary>
/// Options shared across multiple <see cref="LpdfEngine.RenderPdf(string, RenderOptions?)"/> calls.
/// </summary>
public sealed class RenderOptions
{
    /// <summary>
    /// Pre-loaded font bytes for custom fonts referenced via <c>fonts src="…"</c>.
    /// Keys are the font token names used in the document; values are raw TTF/OTF bytes.
    /// </summary>
    public IReadOnlyDictionary<string, byte[]>? FontBytes { get; init; }

    /// <summary>
    /// Pre-loaded image bytes for images referenced via <c>&lt;img name="…"&gt;</c>.
    /// Keys are the image names declared in <c>&lt;assets&gt;</c>; values are raw PNG/JPEG bytes.
    /// </summary>
    public IReadOnlyDictionary<string, byte[]>? ImageBytes { get; init; }

    /// <summary>
    /// File-read callback for resolving font <c>src</c> paths at render time.
    /// On the server this can be set to <c>System.IO.File.ReadAllBytes</c>.
    /// In sandboxed environments supply all bytes via <see cref="FontBytes"/>.
    /// </summary>
    public Func<string, byte[]>? SrcFallback { get; init; }
}

/// <summary>
/// Stateful lpdf renderer.
/// Construct once with a license key; call <see cref="RenderPdf(string, RenderOptions?)"/>
/// or <see cref="RenderPdf(LpdfDocument, RenderOptions?)"/> as many times as needed.
/// </summary>
public sealed class LpdfEngine : IDisposable
{
    private readonly string         _licenseKey;
    private readonly RenderOptions  _opts;
    private readonly WasmRunner     _wasm;
    private readonly Dictionary<string, byte[]> _fonts  = new();
    private readonly Dictionary<string, byte[]> _images = new();
    private          bool           _disposed;

    /// <param name="licenseKey">
    ///   Your lpdf license key. Pass an empty string to render in evaluation
    ///   mode (produces a visible watermark).
    /// </param>
    /// <param name="options">Shared render options applied to every call.</param>
    public LpdfEngine(string licenseKey, RenderOptions? options = null)
    {
        _licenseKey = licenseKey ?? throw new ArgumentNullException(nameof(licenseKey));
        _opts       = options ?? new RenderOptions();
        _wasm       = new WasmRunner();
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// <summary>
    /// Register raw TTF/OTF bytes for a font name referenced via <c>fonts src="…"</c>.
    /// Call before <see cref="RenderPdf(string, RenderOptions?)"/>. Returns <c>this</c>.
    /// </summary>
    public LpdfEngine LoadFont(string name, byte[] bytes)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(bytes);
        _fonts[name] = bytes;
        return this;
    }

    /// <summary>
    /// Register raw PNG or JPEG bytes for an image name referenced via <c>&lt;img name="…"&gt;</c>.
    /// Call before <see cref="RenderPdf(string, RenderOptions?)"/>. Returns <c>this</c>.
    /// </summary>
    public LpdfEngine LoadImage(string name, byte[] bytes)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(bytes);
        _images[name] = bytes;
        return this;
    }

    /// <summary>Render an lpdf XML string to PDF bytes.</summary>
    /// <param name="xml">Full lpdf XML document string.</param>
    /// <param name="callOptions">Per-call overrides merged with the constructor options.</param>
    public Task<byte[]> RenderPdf(string xml, RenderOptions? callOptions = null)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(xml);
        var merged = Merge(callOptions);
        return Task.FromResult(
            _wasm.RenderPdf(xml, _licenseKey, merged.FontBytes, merged.ImageBytes, merged.SrcFallback));
    }

    /// <summary>Render an <see cref="LpdfDocument"/> tree (built with <see cref="LpdfKit"/>) to PDF bytes.</summary>
    /// <param name="document">Document tree produced by <c>LpdfKit.Document(…)</c>.</param>
    /// <param name="callOptions">Per-call overrides merged with the constructor options.</param>
    public Task<byte[]> RenderPdf(LpdfDocument document, RenderOptions? callOptions = null)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(document);
        var merged = Merge(callOptions);
        var json   = JsonSerializer.Serialize(document, LpdfDocumentJson.Options);
        return Task.FromResult(
            _wasm.RenderTreePdf(json, _licenseKey, merged.FontBytes, merged.ImageBytes, merged.SrcFallback));
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    private RenderOptions Merge(RenderOptions? call)
    {
        if (call is null && _fonts.Count == 0 && _images.Count == 0) return _opts;
        return new RenderOptions
        {
            FontBytes   = MergeDicts(MergeDicts(_opts.FontBytes, call?.FontBytes), _fonts.Count > 0 ? _fonts : null),
            ImageBytes  = MergeDicts(MergeDicts(_opts.ImageBytes, call?.ImageBytes), _images.Count > 0 ? _images : null),
            SrcFallback = call?.SrcFallback ?? _opts.SrcFallback,
        };
    }

    private static IReadOnlyDictionary<string, byte[]>? MergeDicts(
        IReadOnlyDictionary<string, byte[]>? a,
        IReadOnlyDictionary<string, byte[]>? b)
    {
        if (a is null) return b;
        if (b is null) return a;
        var merged = new Dictionary<string, byte[]>(a);
        foreach (var (k, v) in b) merged[k] = v;
        return merged;
    }

    private void ThrowIfDisposed()
        => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <summary>Releases the WASM runtime resources held by this engine.</summary>
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _wasm.Dispose();
    }
}

/// <summary>Thrown when the lpdf engine returns a layout or parse error.</summary>
public sealed class LpdfRenderException(string message) : Exception(message);

