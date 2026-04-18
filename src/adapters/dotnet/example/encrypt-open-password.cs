/// encrypt-open-password.cs — render showcase-encryption.xml with RC4-128 encryption,
/// open password required, copy disabled.
///
/// Run (after 'make wasi' and 'dotnet build'):
///   dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj

using Lpdf;

var root    = Path.Combine(AppContext.BaseDirectory, "../../../../../../../example/");
var xmlFile = Path.Combine(AppContext.BaseDirectory, "../../../../../../../docs/examples/showcase-encryption.xml");
const string outputFile = "encrypt-open-password-dotnet.pdf";

var xml = await File.ReadAllTextAsync(xmlFile);

var engine = new LpdfEngine(
    licenseKey: "",   // empty key → free tier (watermark)
    options: new RenderOptions { SrcFallback = File.ReadAllBytes });

// With open password — viewers prompt for "password" before displaying content.
engine.SetEncryption(new EncryptOptions
{
    UserPassword  = "password",
    OwnerPassword = "owner",
    Permissions   = new EncryptPermissions { Copy = false },
});

var bytes = await engine.RenderPdf(xml);

await File.WriteAllBytesAsync(Path.Combine(root, $"result/{outputFile}"), bytes);
Console.WriteLine($"output: {outputFile} ({bytes.Length:N0} bytes)");
