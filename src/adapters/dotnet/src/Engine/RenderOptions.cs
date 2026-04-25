namespace Lpdf.Engine;

/// <summary>
/// Per-call options passed to <see cref="LpdfEngine.RenderPdf(string, RenderOptions?)"/>
/// or <see cref="LpdfEngine.RenderPdf(Lpdf.Kit.Document, RenderOptions?)"/>.
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
    /// Optional ISO 8601 creation timestamp (e.g. <c>"2024-06-01T12:00:00"</c>).
    /// When provided, written as <c>/CreationDate</c> in the PDF info dictionary.
    /// Omitting this keeps builds reproducible (no embedded timestamp).
    /// </summary>
    public string? CreatedOn { get; init; }

    /// <summary>
    /// Optional data object for resolving <c>data-*</c> binding attributes in the
    /// XML template. Pass <see langword="null"/> or omit to render with inline
    /// fallback content. Only applies when rendering an XML string.
    /// </summary>
    public object? Data { get; init; }
}
