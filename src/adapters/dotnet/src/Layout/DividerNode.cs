namespace Lpdf.Layout;

/// <summary>A serialised <c>divider</c> horizontal rule node.</summary>
public sealed record DividerNode(
    Dictionary<string, string> Attrs) : Node
{
    /// <inheritdoc/>
    public override string Type => "divider";
}
