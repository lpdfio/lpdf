/// Minimal end-to-end example: render examples from the project root using LpdfEngine.
///
/// Run (after 'make wasi' and 'dotnet build'):
///   dotnet run --project src/adapters/dotnet/example/LpdfExample.csproj

using Lpdf;

var root = Path.Combine(AppContext.BaseDirectory, "../../../../../../../example/");

var examples = new[] { 
    "example1", 
    "example2", 
};

// init engine
var licenseKey = ""; //await File.ReadAllTextAsync(Path.Combine(root, "test.lic"));
var engine = new LpdfEngine(
    licenseKey: licenseKey,
    options: new RenderOptions { SrcFallback = File.ReadAllBytes });

// load assets (only used if referenced in xml/layout)
engine.LoadFont("montserrat", await File.ReadAllBytesAsync(Path.Combine(root, "assets/fonts/Montserrat-Regular.ttf")));
engine.LoadImage("logo", await File.ReadAllBytesAsync(Path.Combine(root, "assets/images/logo-lpdf.png")));

foreach (var example in examples)
{
    // load xml from file
    var xml = await File.ReadAllTextAsync(Path.Combine(root, $"xml/{example}.xml"));

    // render pdf from xml
    var bytes = await engine.RenderPdf(xml);

    // define output file name
    var outputFile = $"{example}-dotnet.pdf";

    // write pdf to output file
    await File.WriteAllBytesAsync(Path.Combine(root, $"result/{outputFile}"), bytes);

    Console.WriteLine($"output: {outputFile} ({bytes.Length:N0} bytes)");
}
