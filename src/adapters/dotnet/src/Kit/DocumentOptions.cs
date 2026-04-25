namespace Lpdf.Kit;

/// <summary>Document-level attributes applied as defaults to every section.</summary>
public sealed record DocumentOptions(
    string?        Size        = null, string?        Orientation = null,
    string?        Margin      = null, string?        Background  = null,
    DocumentTokens? Tokens     = null, DocumentMeta?  Meta        = null);
