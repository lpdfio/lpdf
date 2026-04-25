namespace Lpdf.Kit;

/// <summary>Design-token overrides applied to the whole document.</summary>
public sealed record DocumentTokens(
    Dictionary<string, string>?   Colors = null,
    Dictionary<string, string>?   Space  = null,
    Dictionary<string, string>?   Grid   = null,
    Dictionary<string, string>?   Border = null,
    Dictionary<string, string>?   Radius = null,
    Dictionary<string, string>?   Width  = null,
    Dictionary<string, string>?   Text   = null,
    Dictionary<string, Font>?     Fonts  = null);
