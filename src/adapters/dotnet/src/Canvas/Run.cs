namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A rich-text run inside a <c>canvas-text</c> node.</summary>
public sealed record Run(
    string  Text,
    string? Font     = null,
    double? Size     = null,
    string? Color    = null);
