namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>text</c> layout primitive.</summary>
public sealed record TextOptions(
    string? Font       = null, string? FontSize  = null, string? TextAlign = null,
    string? Color      = null, string? Bold      = null, string? End       = null,
    string? Repeat     = null, string? Width     = null, string? Height    = null,
    string? Padding    = null, string? Background = null, string? Border   = null,
    string? Radius     = null);
