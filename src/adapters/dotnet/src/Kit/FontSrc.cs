namespace Lpdf.Kit;

/// <summary>Font loaded from a file path or URL supplied via <see cref="Lpdf.Engine.EngineOptions.SrcFallback"/>.</summary>
public sealed record FontSrc(string Src) : Font;
