namespace Lpdf.Layout;

/// <summary>A text content item — either a raw string or a <see cref="SpanNode"/>.</summary>
public interface Content { }

/// <summary>A plain-text run inside a <c>text</c> node. Implicitly convertible from <see cref="string"/>.</summary>
internal sealed record RawText(string Value) : Content
{
    public static implicit operator RawText(string s) => new(s);
}
