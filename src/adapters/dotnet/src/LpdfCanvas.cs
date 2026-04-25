using Lpdf.Canvas;

namespace Lpdf;

/// <summary>
/// Static canvas node builder helpers.
/// All methods return <see cref="CanvasNode"/> instances.
///
/// <example>
/// <code>
/// using Lpdf;
/// using Lpdf.Canvas;
///
/// var layer = LpdfCanvas.Layer([
///     LpdfCanvas.Rect(0, 0, 595, 842, new RectStyle(Fill: "lightgray")),
///     LpdfCanvas.Text(100, 100, "Hello"),
/// ]);
/// </code>
/// </example>
/// </summary>
public static class LpdfCanvas
{
    /// <summary>Build a <c>canvas-layer</c> node containing canvas primitives.</summary>
    public static LayerNode Layer(CanvasNode[]? nodes = null, LayerOptions? options = null)
        => new((nodes ?? []).ToList(), options);

    /// <summary>Build a <c>canvas-rect</c> node.</summary>
    public static RectNode Rect(double x, double y, double w, double h, RectStyle? style = null)
        => new(x, y, w, h, style);

    /// <summary>Build a <c>canvas-line</c> node.</summary>
    public static LineNode Line(double x1, double y1, double x2, double y2, LineStyle? style = null)
        => new(x1, y1, x2, y2, style);

    /// <summary>Build a <c>canvas-ellipse</c> node.</summary>
    public static EllipseNode Ellipse(double cx, double cy, double rx, double ry, EllipseStyle? style = null)
        => new(cx, cy, rx, ry, style);

    /// <summary>Build a <c>canvas-circle</c> node (uniform radii convenience form).</summary>
    public static CircleNode Circle(double cx, double cy, double r, EllipseStyle? style = null)
        => new(cx, cy, r, style);

    /// <summary>Build a <c>canvas-path</c> node from an SVG path string.</summary>
    public static PathNode Path(string d, PathStyle? style = null)
        => new(d, style);

    /// <summary>Build a <c>canvas-text</c> node.</summary>
    public static Canvas.TextNode Text(double x, double y, string content, TextStyle? style = null, Run[]? runs = null)
        => new(x, y, content, style, runs);

    /// <summary>Build a <c>canvas-image</c> node.</summary>
    public static ImageNode Image(double x, double y, string name, double? w = null, double? h = null)
        => new(x, y, name, w, h);
}
