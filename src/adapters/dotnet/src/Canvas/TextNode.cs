namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-text</c> node — positioned text with optional rich-text runs.</summary>
public sealed record TextNode(
    double      X, double Y,
    string      Content,
    TextStyle?  Style = null,
    Run[]?      Runs  = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-text";
}
