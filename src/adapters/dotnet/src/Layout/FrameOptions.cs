namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>frame</c> layout primitive.</summary>
public sealed record FrameOptions(
    string? Width      = null, string? Height     = null, string? Padding    = null,
    string? Background = null, string? Border     = null, string? Radius     = null,
    string? Align      = null, string? Debug      = null);
