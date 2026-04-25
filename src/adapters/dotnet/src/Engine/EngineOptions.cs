namespace Lpdf.Engine;

/// <summary>
/// Construction-time configuration for <see cref="LpdfEngine"/>.
/// Passed once at construction; applies to every subsequent render call.
/// </summary>
public sealed class EngineOptions
{
    /// <summary>
    /// File-read callback for resolving font <c>src</c> paths at render time.
    /// On the server this can be set to <c>System.IO.File.ReadAllBytes</c>.
    /// In sandboxed environments supply all bytes via
    /// <see cref="RenderOptions.FontBytes"/> instead.
    /// </summary>
    public Func<string, byte[]>? SrcFallback { get; init; }
}
