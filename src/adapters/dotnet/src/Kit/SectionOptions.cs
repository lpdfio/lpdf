namespace Lpdf.Kit;

/// <summary>Attributes for the <c>section</c> (page) element.</summary>
public sealed record SectionOptions(
    string? Size        = null, string? Orientation = null, string? Margin   = null,
    string? Background  = null, string? Debug       = null);
