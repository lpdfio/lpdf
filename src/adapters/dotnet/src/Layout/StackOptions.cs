namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>stack</c> layout primitive.</summary>
public sealed record StackOptions(
    string? Gap        = null, string? Padding = null, string? Background = null,
    string? Align      = null, string? Justify = null, string? Width      = null,
    string? Height     = null, string? Border  = null, string? Radius     = null,
    string? Debug      = null);
