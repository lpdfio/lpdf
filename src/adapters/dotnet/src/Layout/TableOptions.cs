namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>table</c> layout primitive.</summary>
public sealed record TableOptions(
    string? Cols       = null, string? Border    = null, string? Stripe     = null,
    string? Gap        = null, string? Padding   = null, string? Background = null,
    string? Width      = null, string? Height    = null, string? Repeat     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>thead</c> table header row group.</summary>
public sealed record TheadOptions(
    string? Background = null);

/// <summary>Attributes for the <c>tr</c> table row.</summary>
public sealed record TrOptions(
    string? Background = null);

/// <summary>Attributes for the <c>td</c> table cell.</summary>
public sealed record TdOptions(
    string? Padding    = null, string? Background = null,
    string? Align      = null, string? Valign     = null,
    string? Border     = null, string? Radius     = null,
    string? Gap        = null, string? Debug      = null);
