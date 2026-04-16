/// Minimal end-to-end example: render invoice.xml from the project root using LpdfEngine.
///
/// Run (after 'make wasi' and 'dotnet build'):
///   dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj
///
/// Output: example/invoice-dotnet.pdf written to the project root.

using Lpdf;

// init engine
var engine = new LpdfEngine(
    licenseKey: "",   // empty → evaluation watermark
    options: new RenderOptions { SrcFallback = File.ReadAllBytes });

// optional: load fonts and assets

var inputFile  = "invoice.xml";
var outputFile = "invoice-dotnet.pdf";

// load xml from file
var xml = await File.ReadAllTextAsync(Path.Combine("example", inputFile));

// render pdf from xml
var bytes = await engine.RenderPdf(xml);

// write pdf to file
// Directory.CreateDirectory("example");
await File.WriteAllBytesAsync(Path.Combine("example", outputFile), bytes);

Console.WriteLine($"output: {outputFile} ({bytes.Length:N0} bytes)");
