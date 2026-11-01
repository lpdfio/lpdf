// Compiles the C# that the codegen's `dotnet` target writes for every fixture in test/fixtures, against the .NET SDK,
// so a generator change that writes code the SDK doesn't take fails here and not in a user's project. Nothing else
// compiles that output: VS Code's "generate code" shows it as it is.
//
// Each fixture's output is the Program.cs of a console project of its own, as `dotnet new console` makes one
// (implicit usings, nullable on), that references the SDK's project. One solution builds them all at once. Only the
// compile is checked: nothing is run.
//
// The projects are made in a temporary folder outside the checkout: in the checkout, an editor's C# tooling loads each
// new project as it appears and builds it in the background, into the folders this script makes and removes.
//
// The SDK's project embeds the WASI engine, so its `wasm/lpdf.wasm` must be there: `make check-codegen-dotnet` builds
// the CLI and the engine and copies it in.
//
// Usage: node scripts/check-codegen-dotnet.mjs <lpdf CLI> <lpdf-dotnet folder>     list the failures, exit 1 if any
//        needs the .NET 8 SDK or later on the PATH

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [cliArg, sdkArg] = process.argv.slice(2);
if (!cliArg || !sdkArg) {
    console.error('usage: node scripts/check-codegen-dotnet.mjs <lpdf CLI> <lpdf-dotnet folder>');
    process.exit(2);
}

const root = fileURLToPath(new URL('..', import.meta.url));
const cli = process.platform === 'win32' && !cliArg.endsWith('.exe') ? `${cliArg}.exe` : cliArg;
const sdkProject = path.resolve(sdkArg, 'Lpdf.csproj');
const fixtures = path.join(root, 'test', 'fixtures');

for (const [file, what] of [[cli, 'the lpdf CLI'], [sdkProject, 'the .NET SDK project'], [path.resolve(sdkArg, 'wasm', 'lpdf.wasm'), "the SDK's engine"]]) {
    if (!existsSync(file)) {
        console.error(`${what} is not at ${file}`);
        process.exit(2);
    }
}

/** Runs a command; a non-zero exit is data here, not an error. */
function run(command, args, options = {}) {
    const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
    if (result.error) {
        console.error(`could not run ${command}: ${result.error.message}`);
        process.exit(2);
    }
    return { status: result.status, output: `${result.stdout}${result.stderr}` };
}

const work = mkdtempSync(path.join(tmpdir(), 'lpdf-codegen-dotnet-'));

const failures = [];
const projects = [];

const files = readdirSync(fixtures).filter((name) => name.endsWith('.xml')).sort();
for (const file of files) {
    const name = path.basename(file, '.xml');
    const folder = path.join(work, name);
    mkdirSync(folder);

    const generated = run(cli, ['codegen', '--input', path.join(fixtures, file), '--target', 'dotnet', '--output', path.join(folder, 'Program.cs')]);
    if (generated.status !== 0) {
        failures.push(`${name}: codegen failed: ${generated.output.trim()}`);
        continue;
    }

    const project = path.join(folder, `${name}.csproj`);
    writeFileSync(project, `<Project Sdk="Microsoft.NET.Sdk">
    <PropertyGroup>
        <OutputType>Exe</OutputType>
        <TargetFramework>net8.0</TargetFramework>
        <ImplicitUsings>enable</ImplicitUsings>
        <Nullable>enable</Nullable>
    </PropertyGroup>
    <ItemGroup>
        <ProjectReference Include="${sdkProject}" />
    </ItemGroup>
</Project>
`);
    projects.push(project);
}

// `dotnet new sln` writes all.sln with the .NET 8 SDK and all.slnx with later ones; both build the same.
const created = run('dotnet', ['new', 'sln', '--name', 'all', '--output', work]);
const solution = readdirSync(work).find((name) => /^all\.slnx?$/.test(name));
if (!solution) {
    console.error(`dotnet new sln made no solution: ${created.output.trim()}`);
    process.exit(2);
}
const added = run('dotnet', ['sln', path.join(work, solution), 'add', ...projects]);
if (added.status !== 0) {
    console.error(`dotnet sln add failed: ${added.output.trim()}`);
    process.exit(2);
}

const built = run('dotnet', ['build', path.join(work, solution), '-m', '-nologo', '-v', 'q']);
if (built.status !== 0) {
    // The first error in each fixture's Program.cs, and how many there are; MSBuild repeats each one in its summary.
    const errors = new Set(built.output.split('\n').filter((line) => / error [A-Z]+\d+:/.test(line)).map((line) => line.trim()));
    const byFixture = new Map();
    for (const line of errors) {
        const match = line.match(/[\\/]([^\\/]+)[\\/]Program\.cs(\(\d+,\d+\): error .*?)(\s+\[[^\]]+\])?$/);
        const [fixture, error] = match ? [match[1], match[2]] : ['(build)', line];
        byFixture.set(fixture, [...(byFixture.get(fixture) ?? []), error]);
    }
    for (const [fixture, list] of byFixture) {
        failures.push(`${fixture}${list[0]}${list.length > 1 ? ` (and ${list.length - 1} more)` : ''}`);
    }
    if (byFixture.size === 0) failures.push(`dotnet build failed: ${built.output.trim()}`);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    console.error(`${failures.length} of ${files.length} fixtures fail; the projects are in ${work}`);
    process.exit(1);
}
rmSync(work, { recursive: true, force: true });
console.log(`C# check passed: the dotnet output of ${projects.length} fixtures compiles against ${sdkProject}`);
