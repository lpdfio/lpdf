using System.Text.Json;
using Lpdf.Engine;
using Lpdf.Kit;
using Lpdf.Shared;

namespace Lpdf;

/// <summary>
/// Stateful lpdf renderer.
/// Construct once with a license key; call <see cref="RenderPdf(string, RenderOptions?)"/>
/// or <see cref="RenderPdf(Document, RenderOptions?)"/> as many times as needed.
/// </summary>
public sealed class LpdfEngine : IDisposable
{
    private readonly string         _licenseKey;
    private readonly EngineOptions  _engineOpts;
    private readonly WasmRunner     _wasm;
    private readonly Dictionary<string, byte[]> _fonts  = new();
    private readonly Dictionary<string, byte[]> _images = new();
    private          bool           _disposed;
    private          EncryptOptions? _encrypt;

    /// <param name="licenseKey">
    ///   Your lpdf license key. Pass an empty string to render in evaluation
    ///   mode (produces a visible watermark).
    /// </param>
    /// <param name="options">Engine-level options applied to every call (e.g. <c>SrcFallback</c>).</param>
    public LpdfEngine(string licenseKey, EngineOptions? options = null)
    {
        _licenseKey = licenseKey ?? throw new ArgumentNullException(nameof(licenseKey));
        _engineOpts = options ?? new EngineOptions();
        _wasm       = new WasmRunner();
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// <summary>
    /// Configure RC4-128 encryption for all subsequent <see cref="RenderPdf(string, RenderOptions?)"/> calls.
    /// Pass <see langword="null"/> to clear previously set encryption.
    /// </summary>
    public LpdfEngine SetEncryption(EncryptOptions? options)
    {
        ThrowIfDisposed();
        _encrypt = options;
        return this;
    }

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
    /// <param name="callOptions">Per-call options (data, fonts, images, timestamp).</param>
    public Task<byte[]> RenderPdf(string xml, RenderOptions? callOptions = null)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(xml);
        var (fontBytes, imageBytes) = MergeAssets(callOptions);
        var encryptJson = BuildEncryptJson();
        var dataJson    = BuildDataJson(callOptions?.Data);
        return Task.Run(() =>
            _wasm.RenderPdf(xml, _licenseKey, fontBytes, imageBytes,
                _engineOpts.SrcFallback, encryptJson, callOptions?.CreatedOn, dataJson));
    }

    /// <summary>Render a <see cref="Document"/> tree (built with <see cref="LpdfKit"/>) to PDF bytes.</summary>
    /// <param name="document">Document tree produced by <c>LpdfKit.Document(…)</c>.</param>
    /// <param name="callOptions">Per-call options (fonts, images, timestamp).</param>
    public Task<byte[]> RenderPdf(Document document, RenderOptions? callOptions = null)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(document);
        var json = JsonSerializer.Serialize(document, DocumentJson.Options);
        var (fontBytes, imageBytes) = MergeAssets(callOptions);
        var encryptJson = BuildEncryptJson();
        return Task.Run(() =>
            _wasm.RenderTreePdf(json, _licenseKey, fontBytes, imageBytes,
                _engineOpts.SrcFallback, encryptJson, callOptions?.CreatedOn));
    }

    /// <summary>
    /// Convert a <see cref="Document"/> tree (built with <see cref="LpdfKit"/>) to an lpdf XML string.
    /// </summary>
    /// <param name="document">Document tree produced by <c>LpdfKit.Document(…)</c>.</param>
    /// <returns>A well-formed XML string with an <c>&lt;?xml ...?&gt;</c> declaration.</returns>
    public Task<string> KitToXml(Document document)
    {
        ArgumentNullException.ThrowIfNull(document);
        var json = JsonSerializer.Serialize(document, DocumentJson.Options);
        return Task.Run(() => _wasm.KitToXml(json));
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    private string? BuildEncryptJson()
    {
        if (_encrypt is null) return null;
        var p = _encrypt.Permissions;
        return JsonSerializer.Serialize(new
        {
            user_password  = _encrypt.UserPassword,
            owner_password = _encrypt.OwnerPassword,
            permissions    = new
            {
                print         = p.Print,
                modify        = p.Modify,
                copy          = p.Copy,
                annotate      = p.Annotate,
                fill_forms    = p.FillForms,
                accessibility = p.Accessibility,
                assemble      = p.Assemble,
                print_hq      = p.PrintHq,
            },
        });
    }

    private static string? BuildDataJson(object? data)
    {
        if (data is null) return null;
        return JsonSerializer.Serialize(data);
    }

    private (IReadOnlyDictionary<string, byte[]>? fonts, IReadOnlyDictionary<string, byte[]>? images)
        MergeAssets(RenderOptions? call)
    {
        var fonts  = MergeDicts(call?.FontBytes, _fonts.Count > 0 ? _fonts : null);
        var images = MergeDicts(call?.ImageBytes, _images.Count > 0 ? _images : null);
        return (fonts, images);
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

