namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>span</c> inline primitive.</summary>
public sealed record SpanOptions(
    string? Font       = null, string? FontSize  = null, string? Color     = null,
    string? Bold       = null, string? Url       = null, string? Underline = null,
    string? Strike     = null);
