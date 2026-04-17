using Xunit;

namespace Lpdf.Tests;

/// <summary>
/// Snapshot tests: render each fixture XML → PDF, SHA-256 the bytes, compare
/// against the stored golden value in <c>test/snapshots/</c>.
///
/// The snapshot files are shared with the Node adapter — byte-identical output
/// from the same Rust core means the hashes should match across adapters.
///
/// To regenerate hashes after an intentional rendering change:
/// <code>
///   UPDATE_SNAPSHOTS=1 dotnet test src/adapters/dotnet
/// </code>
/// </summary>
public class SnapshotTests
{
    [Theory]
    [InlineData("example1")]
    [InlineData("example2")]
    [InlineData("example3")]
    [InlineData("example4")]
    [InlineData("example5")]
    [InlineData("example6")]
    [InlineData("example7")]
    [InlineData("example8")]
    [InlineData("example9")]
    [InlineData("example10")]
    [InlineData("example11")]
    [InlineData("showcase-cluster")]
    [InlineData("showcase-flank")]
    [InlineData("showcase-frame")]
    [InlineData("showcase-grid")]
    [InlineData("showcase-split")]
    [InlineData("showcase-stack")]
    public async Task FixtureMatchesStoredHash(string name)
    {
        var xml   = File.ReadAllText(Path.Combine(SnapshotHelper.Fixtures, $"{name}.xml"));
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml);
        SnapshotHelper.CompareOrUpdate(name, bytes);
    }
}
