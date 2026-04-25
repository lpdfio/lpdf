using Lpdf.Engine;
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
    [InlineData("showcase-table")]
    [InlineData("showcase-barcode")]
    [InlineData("showcase-encryption")]
    [InlineData("showcase-forms")]
    [InlineData("bench_xs")]
    [InlineData("bench_s")]
    [InlineData("bench_m")]
    [InlineData("bench_l")]
    [InlineData("bench_xl")]
    public async Task FixtureMatchesStoredHash(string name)
    {
        var xml   = File.ReadAllText(Path.Combine(SnapshotHelper.Fixtures, $"{name}.xml"));
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml);
        SnapshotHelper.CompareOrUpdate(name, bytes);
    }
}

/// <summary>
/// Engine feature tests: encryption and image loading.
/// </summary>
public class EngineFeatureTests
{
    private static readonly string _minimalXml =
        "<lpdf version=\"1\"><document><section><layout></layout></section></document></lpdf>";

    [Fact]
    public async Task SetEncryption_ProducesEncryptedPdf()
    {
        using var engine = new LpdfEngine("test-key");
        engine.SetEncryption(new EncryptOptions
        {
            UserPassword  = "",
            OwnerPassword = "s3cr3t",
        });
        var bytes = await engine.RenderPdf(_minimalXml);

        Assert.True(
            bytes[..5].SequenceEqual(System.Text.Encoding.ASCII.GetBytes("%PDF-")),
            "Output must start with %PDF-");

        // Encrypted PDFs contain the /Encrypt dictionary entry
        var text = System.Text.Encoding.Latin1.GetString(bytes);
        Assert.Contains("/Encrypt", text);
    }

    [Fact]
    public async Task LoadImage_DoesNotThrowAndProducesValidPdf()
    {
        // A minimal 1×1 white grayscale PNG
        var png1x1 = Convert.FromHexString(
            "89504e470d0a1a0a0000000d49484452000000010000000108000000003a7e9b55" +
            "0000000a49444154789c6260000000020001e221bc330000000049454e44ae426082");

        using var engine = new LpdfEngine("test-key");
        engine.LoadImage("testimg", png1x1);
        var bytes = await engine.RenderPdf(_minimalXml);

        Assert.True(
            bytes[..5].SequenceEqual(System.Text.Encoding.ASCII.GetBytes("%PDF-")),
            "Output must start with %PDF-");
    }
}
