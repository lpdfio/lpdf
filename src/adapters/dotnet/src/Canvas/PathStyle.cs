namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Style for <c>canvas-path</c> nodes.</summary>
public sealed record PathStyle(
    string?  Fill        = null,
    string?  Stroke      = null,
    double?  StrokeWidth = null,
    double?  StrokeDash  = null);
