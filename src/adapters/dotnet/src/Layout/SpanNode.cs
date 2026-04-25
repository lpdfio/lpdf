namespace Lpdf.Layout;

/// <summary>A serialised <c>span</c> inline node.</summary>
public sealed record SpanNode(
    Dictionary<string, string> Attrs,
    List<string>               Children) : Node, Content
{
    /// <inheritdoc/>
    public override string Type => "span";
}
