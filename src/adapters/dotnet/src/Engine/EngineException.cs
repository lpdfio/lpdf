namespace Lpdf.Engine;

/// <summary>Thrown when the lpdf engine returns a layout or parse error.</summary>
public sealed class EngineException(string message) : Exception(message);
