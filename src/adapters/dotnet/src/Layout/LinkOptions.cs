namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>link</c> layout primitive.</summary>
public sealed record LinkOptions(
    string? Url        = null, string? Width = null, string? Height = null);
