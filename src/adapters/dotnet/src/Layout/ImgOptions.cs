namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>img</c> primitive.</summary>
public sealed record ImgOptions(
    string  Name,
    string? Height     = null, string? Width      = null,
    string? Font       = null, string? FontSize   = null,
    string? Gap        = null, string? Padding    = null,
    string? Background = null, string? Border     = null,
    string? Radius     = null, string? Repeat     = null,
    string? Debug      = null);
