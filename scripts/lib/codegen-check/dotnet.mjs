// The check for the .NET SDK (`Lpdfio.Lpdf`): one console project that references the SDK, and a program in it that runs
// every document.
//
// The code the generator writes is a program of top-level statements: the `using` lines, the engine, the document as
// `var doc = …;` and the render. Each document becomes a static class of its own in a file of its own, `Doc<n>`, with
// the statements that build the document as the body of its `Build` method, so that a compile error names the document.
// The program calls each `Build` by name, and renders the document and the XML through one engine.
//
// A document that doesn't compile is left out and the rest is built again, until every one that is left compiles.

import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { firstLine, missingResults, readResults, run as exec, writeDocuments } from './common.mjs';

export const target = 'dotnet';

const PROGRAM = /^((?:using [^\n]+;\n)+)\n+var engine = [^\n]*;\n\n([\s\S]*?)\n\nvar pdf = [^\n]*\n?$/;

/** The source of the class of one document, or the reason its code is not in the shape of a program. */
export function shape(index, code) {
    const match = PROGRAM.exec(code);
    if (!match) throw new Error('the program is not the using lines, the engine, `var doc = …;` and `var pdf = …;`');
    return `${match[1]}
public static class Doc${index}
{
    public static Lpdf.Kit.PdfDocument Build()
    {
${match[2]}
        return doc;
    }
}
`;
}

const PROJECT = (sdk) => `<Project Sdk="Microsoft.NET.Sdk">
    <PropertyGroup>
        <OutputType>Exe</OutputType>
        <TargetFramework>net8.0</TargetFramework>
        <RollForward>Major</RollForward>
        <ImplicitUsings>enable</ImplicitUsings>
        <Nullable>enable</Nullable>
        <NoWarn>$(NoWarn);CS8981</NoWarn>
    </PropertyGroup>
    <ItemGroup>
        <ProjectReference Include="${sdk}" />
    </ItemGroup>
</Project>
`;

const CHECK = (createdOn) => `using System.Reflection;
using Lpdf;
using Lpdf.Engine;

static class Check
{
    static string First(string? message) => (message ?? "").Split('\\n').Select(line => line.Trim()).FirstOrDefault(line => line.Length > 0) ?? "";

    static (string, string) Run(string work, string index)
    {
        var engine = L.Engine();
        foreach (var line in File.ReadLines(Path.Combine(work, "docs", index + ".assets")))
        {
            var parts = line.Split('\\t');
            var bytes = File.ReadAllBytes(parts[2]);
            switch (parts[0])
            {
                case "font": engine.LoadFont(parts[1], bytes); break;
                case "image": engine.LoadImage(parts[1], bytes); break;
                case "file": engine.LoadFile(parts[1], bytes); break;
                case "profile": engine.LoadProfile(parts[1], bytes); break;
            }
        }
        var options = new RenderOptions { CreatedOn = "${createdOn}", SkipEncryption = true };
        byte[] fromXml;
        try
        {
            fromXml = engine.Render(File.ReadAllText(Path.Combine(work, "docs", index + ".xml")), options);
        }
        catch (Exception e)
        {
            return ("xml", First(e.Message));
        }
        try
        {
            var build = Type.GetType("Doc" + index)!.GetMethod("Build")!;
            var document = (Lpdf.Kit.PdfDocument)build.Invoke(null, null)!;
            var fromCode = engine.Render(document, options);
            return (fromXml.AsSpan().SequenceEqual(fromCode) ? "ok" : "differs", "");
        }
        catch (Exception e)
        {
            return ("code", First((e as TargetInvocationException)?.InnerException?.Message ?? e.Message));
        }
    }

    static int Main(string[] args)
    {
        var work = args[0];
        foreach (var index in File.ReadLines(Path.Combine(work, "indexes.txt")))
        {
            var (status, message) = Run(work, index);
            Console.Out.Write($"RESULT\\t{index}\\t{status}\\t{message}\\n");
            Console.Out.Flush();
        }
        return 0;
    }
}
`;

export async function run({ sdk, work, base, placeholders, documents, createdOn }) {
    const results = new Map();
    const project = path.join(work, 'check');
    const source = path.join(project, 'src');
    mkdirSync(source, { recursive: true });
    writeFileSync(path.join(project, 'check.csproj'), PROJECT(path.resolve(sdk, 'Lpdf.csproj')));
    writeFileSync(path.join(source, 'Check.cs'), CHECK(createdOn));
    writeDocuments(work, documents, base, placeholders);

    const buildable = new Map();
    for (const { index, code } of documents) {
        try {
            writeFileSync(path.join(source, `Doc${index}.cs`), shape(index, code));
            buildable.set(index, true);
        } catch (e) {
            results.set(index, { status: 'shape', message: firstLine(e.message) });
        }
    }

    // MSBuild names the file of each error: Doc12.cs(3,14): error CS1061: …
    for (let round = 0; buildable.size > 0; round++) {
        const built = exec('dotnet', ['build', path.join(project, 'check.csproj'), '-c', 'Release', '-o', path.join(project, 'out'), '-nologo', '-v', 'q', '-clp:NoSummary']);
        if (built.status === 0) break;
        const errors = [...new Set((built.stdout + built.stderr).split('\n').filter((line) => / error [A-Z]+\d+:/.test(line)).map((line) => line.trim()))];
        const failed = new Map();
        for (const line of errors) {
            const match = line.match(/[\\/]Doc(\d+)\.cs\(\d+,\d+\): error ([A-Z]+\d+): (.*?)(?:\s+\[[^\]]+\])?$/);
            if (match && buildable.has(Number(match[1])) && !failed.has(Number(match[1]))) failed.set(Number(match[1]), `${match[2]}: ${match[3]}`);
        }
        if (failed.size === 0 || round >= 20) {
            // An error in the project or the program, or a build that does not settle: nothing can run.
            for (const index of buildable.keys()) results.set(index, { status: 'compile', message: firstLine(errors[0] ?? built.stdout + built.stderr) });
            buildable.clear();
            break;
        }
        for (const [index, message] of failed) {
            results.set(index, { status: 'compile', message });
            buildable.delete(index);
            rmSync(path.join(source, `Doc${index}.cs`));
        }
    }
    if (buildable.size === 0) return results;

    writeFileSync(path.join(work, 'indexes.txt'), [...buildable.keys()].join('\n') + '\n');
    const ran = exec('dotnet', [path.join(project, 'out', 'check.dll'), work]);
    for (const [index, result] of readResults(ran.stdout)) results.set(index, result);
    return missingResults(results, documents, ran.stdout + ran.stderr);
}
