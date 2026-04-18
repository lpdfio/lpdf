/// encrypt-permissions-only.cs — render showcase-encryption.xml with RC4-128 encryption,
/// no open password, print and copy disabled.
///
/// Run (after 'make wasi' and 'dotnet build'):
///   dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj

using Lpdf;

var root    = Path.Combine(AppContext.BaseDirectory, "../../../../../../../example/");
var xmlFile = Path.Combine(AppContext.BaseDirectory, "../../../../../../../docs/examples/showcase-encryption.xml");
const string outputFile = "encrypt-permissions-only-dotnet.pdf";

var xml = await File.ReadAllTextAsync(xmlFile);

var engine = new LpdfEngine(
    licenseKey: "",   // empty key → free tier (watermark)
    options: new RenderOptions { SrcFallback = File.ReadAllBytes });

// Permissions only — no open password.
// File opens freely; cooperative viewers enforce Print = false, Copy = false.
engine.SetEncryption(new EncryptOptions
{
    UserPassword  = "",
    OwnerPassword = "s3cr3t",
    Permissions   = new EncryptPermissions { Print = false, Copy = false },
});

var bytes = await engine.RenderPdf(xml);

await File.WriteAllBytesAsync(Path.Combine(root, $"result/{outputFile}"), bytes);
Console.WriteLine($"output: {outputFile} ({bytes.Length:N0} bytes)");
