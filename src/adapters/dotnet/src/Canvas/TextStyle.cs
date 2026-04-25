namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Style for <c>canvas-text</c> nodes.</summary>
public sealed record TextStyle(
    string?    Font       = null,
    double?    Size       = null,
    string?    Color      = null,
    TextAlign? Align      = null,
    double?    LineHeight = null,
    double?    Width      = null);
