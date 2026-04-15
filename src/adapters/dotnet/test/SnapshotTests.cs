using System.Security.Cryptography;
using System.Text;
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
    private static readonly string Root      = FindRoot();
    private static readonly string Fixtures  = Path.Combine(Root, "test", "fixtures");
    private static readonly string Snapshots = Path.Combine(Root, "test", "snapshots");
    private static readonly bool   Update    = Environment.GetEnvironmentVariable("UPDATE_SNAPSHOTS") == "1";

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
    public async Task FixtureMatchesStoredHash(string name)
    {
        var xml   = File.ReadAllText(Path.Combine(Fixtures, $"{name}.xml"));
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml);

        Assert.True(
            bytes[..5].SequenceEqual(Encoding.ASCII.GetBytes("%PDF-")),
            "Output must start with %PDF-");

        var hash = Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
        var snap = Path.Combine(Snapshots, $"{name}.pdf.sha256");

        if (Update)
        {
            File.WriteAllText(snap, hash);
        }
        else
        {
            var stored = File.ReadAllText(snap).Trim();
            Assert.Equal(stored, hash);
        }
    }

    private static string FindRoot()
    {
        // Walk up from the test assembly until we find Cargo.toml (project root).
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null && !File.Exists(Path.Combine(dir.FullName, "Cargo.toml")))
            dir = dir.Parent;
        return dir?.FullName
            ?? throw new InvalidOperationException("Could not locate project root (Cargo.toml not found).");
    }
}
