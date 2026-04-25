namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>grid</c> layout primitive.</summary>
public sealed record GridOptions(
    string? Cols       = null, string? ColWidth = null, string? Gap        = null,
    string? Equal      = null, string? Padding  = null, string? Background = null,
    string? Width      = null, string? Height   = null, string? Border     = null,
    string? Radius     = null, string? Debug    = null);
