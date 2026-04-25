namespace Lpdf.Kit;

/// <summary>PDF document metadata written into the output file.</summary>
public sealed record DocumentMeta(
    string? Title    = null, string? Author   = null, string? Subject  = null,
    string? Keywords = null, string? Creator  = null);
