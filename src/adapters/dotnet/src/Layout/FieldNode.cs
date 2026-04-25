namespace Lpdf.Layout;

/// <summary>A serialised form <c>field</c> node.</summary>
public sealed record FieldNode(
    Dictionary<string, string> Attrs) : Node
{
    /// <inheritdoc/>
    public override string Type => "field";
}
