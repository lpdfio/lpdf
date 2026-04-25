namespace Lpdf.Layout;

/// <summary>A serialised <c>img</c> image node.</summary>
public sealed record ImgNode(
    Dictionary<string, string> Attrs) : Node
{
    /// <inheritdoc/>
    public override string Type => "img";
}
