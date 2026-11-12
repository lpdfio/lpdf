// The check for the Java SDK (`io.lpdf:lpdf`): one program that runs every document, compiled against the SDK's jar.
//
// The code the generator writes is a class `Main`, whose `main` has the engine, the document as `var doc = …;` and the
// render, and for a large document the methods that hold its sections. Each document becomes a class of its own,
// `Doc<n>`, in a file of its own, whose `build` is that `main` returning the document, so that a compile error names
// the document and the JVM's limit of 64 KB to a method is the generator's own to keep. The program calls each `build`
// by name, and renders the document and the XML through one engine.
//
// The SDK is built first (`mvn package`), which leaves its jar in `target`; its runtime dependencies come from Maven.
// A document that doesn't compile is left out and the rest is compiled again, until every one that is left compiles.

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { fail, firstLine, missingResults, readResults, run as exec, writeDocuments } from './common.mjs';

export const target = 'java';

/** The three places `main` of the generated class is rewritten at: its head, its end, and the name of the class. */
const HEAD = /    public static void main\(String\[\] args\) \{\n        var engine = [^\n]*;\n\n/;
const TAIL = /\n\n        byte\[\] pdf = engine\.render\(doc\);\n    \}/;
const CLASS = /\npublic class Main \{\n/;

/** The source of the class of one document, or the reason its code is not in the shape of a program. */
export function shape(index, code) {
    if (![HEAD, TAIL, CLASS].every((part) => part.test(code))) {
        throw new Error('the program is not a class Main with a main that has the engine, `var doc = …;` and `byte[] pdf = …;`');
    }
    return code
        .replace(CLASS, `\npublic class Doc${index} {\n`)
        .replace(HEAD, '    public static PdfDocument build() {\n')
        .replace(TAIL, '\n\n        return doc;\n    }');
}

const CHECK = (createdOn) => `import io.lpdf.*;
import java.lang.reflect.InvocationTargetException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;

public class Check {
    static String first(String message) {
        if (message == null) return "";
        for (String line : message.split("\\n")) {
            if (!line.isBlank()) return line.strip();
        }
        return "";
    }

    static String[] run(Path work, String index) throws Exception {
        // 10 seconds, not the SDK's 5: the large benchmark fixtures take longer than 5 on JDK 25 on CI's runners.
        var engine = L.engine(new EngineOptions().timeout(10));
        for (String line : Files.readAllLines(work.resolve("docs/" + index + ".assets"))) {
            String[] parts = line.split("\\t");
            byte[] bytes = Files.readAllBytes(Path.of(parts[2]));
            switch (parts[0]) {
                case "font" -> engine.loadFont(parts[1], bytes);
                case "image" -> engine.loadImage(parts[1], bytes);
                case "file" -> engine.loadFile(parts[1], bytes);
                case "profile" -> engine.loadProfile(parts[1], bytes);
                default -> throw new IllegalStateException("unknown asset " + parts[0]);
            }
        }
        var options = new RenderOptions().createdOn("${createdOn}").skipEncryption(true);
        byte[] fromXml;
        try {
            fromXml = engine.render(Files.readString(work.resolve("docs/" + index + ".xml")), options);
        } catch (Exception e) {
            return new String[] {"xml", first(e.getMessage())};
        }
        try {
            var document = (PdfDocument) Class.forName("Doc" + index).getMethod("build").invoke(null);
            byte[] fromCode = engine.render(document, options);
            return new String[] {Arrays.equals(fromXml, fromCode) ? "ok" : "differs", ""};
        } catch (InvocationTargetException e) {
            return new String[] {"code", first(e.getCause().toString())};
        } catch (Exception e) {
            return new String[] {"code", first(e.toString())};
        }
    }

    public static void main(String[] args) throws Exception {
        Path work = Path.of(args[0]);
        for (String index : Files.readAllLines(work.resolve("indexes.txt"))) {
            String[] result = run(work, index);
            System.out.print("RESULT\\t" + index + "\\t" + result[0] + "\\t" + result[1].replace('\\t', ' ') + "\\n");
            System.out.flush();
        }
    }
}
`;

/** The jar the SDK's build left in its target folder: not the one of the sources, nor the Javadoc. */
function sdkJar(sdk) {
    const target = path.join(sdk, 'target');
    const jar = readdirSync(target, { withFileTypes: true }).map((entry) => entry.name).find((name) => /^lpdf-.*\.jar$/.test(name) && !/-(sources|javadoc)\.jar$/.test(name));
    if (!jar) fail(`the SDK is not built: ${target} has no jar (run mvn package in ${sdk})`);
    return path.join(target, jar);
}

export async function run({ sdk, work, base, placeholders, documents, createdOn }) {
    const results = new Map();
    const source = path.join(work, 'src');
    const out = path.join(work, 'out');
    mkdirSync(source, { recursive: true });
    mkdirSync(out, { recursive: true });

    const dependencies = path.join(work, 'classpath.txt');
    const resolved = exec('mvn', ['-B', '-q', '-f', path.join(sdk, 'pom.xml'), 'dependency:build-classpath', `-Dmdep.outputFile=${dependencies}`, '-DincludeScope=runtime']);
    if (resolved.status !== 0) fail(`mvn could not resolve the SDK's dependencies: ${firstLine(resolved.stdout + resolved.stderr)}`);
    const classpath = [sdkJar(sdk), ...readFileSync(dependencies, 'utf8').trim().split(path.delimiter)].join(path.delimiter);

    writeFileSync(path.join(source, 'Check.java'), CHECK(createdOn));
    writeDocuments(work, documents, base, placeholders);
    const buildable = new Set();
    for (const { index, code } of documents) {
        try {
            writeFileSync(path.join(source, `Doc${index}.java`), shape(index, code));
            buildable.add(index);
        } catch (e) {
            results.set(index, { status: 'shape', message: firstLine(e.message) });
        }
    }

    // javac names the file of each error: …/Doc12.java:34: error: …
    for (let round = 0; buildable.size > 0; round++) {
        const files = ['Check.java', ...[...buildable].map((index) => `Doc${index}.java`)].map((name) => path.join(source, name));
        writeFileSync(path.join(work, 'sources.txt'), files.join('\n') + '\n');
        const compiled = exec('javac', ['-J-Xmx6g', '--release', '17', '-Xmaxerrs', '100000', '-nowarn', '-proc:none', '-d', out, '-cp', classpath, `@${path.join(work, 'sources.txt')}`]);
        if (compiled.status === 0) break;
        const output = compiled.stdout + compiled.stderr;
        const failed = new Map();
        // The error, and for "cannot find symbol" the symbol, three lines on: the line of code, a caret, `symbol: …`.
        for (const match of output.matchAll(/[\\/]Doc(\d+)\.java:\d+: error: ([^\n]*)(?:\n[^\n]*\n[^\n]*\n\s*symbol:\s*([^\n]*))?/g)) {
            if (buildable.has(Number(match[1])) && !failed.has(Number(match[1]))) failed.set(Number(match[1]), match[3] ? `${match[2]}: ${match[3]}` : match[2]);
        }
        if (failed.size === 0 || round >= 20) {
            for (const index of buildable) results.set(index, { status: 'compile', message: firstLine(output) });
            buildable.clear();
            break;
        }
        for (const [index, message] of failed) {
            results.set(index, { status: 'compile', message });
            buildable.delete(index);
            rmSync(path.join(source, `Doc${index}.java`));
        }
    }
    if (buildable.size === 0) return results;

    writeFileSync(path.join(work, 'indexes.txt'), [...buildable].join('\n') + '\n');
    const ran = exec('java', ['-cp', [out, classpath].join(path.delimiter), 'Check', work]);
    for (const [index, result] of readResults(ran.stdout)) results.set(index, result);
    return missingResults(results, documents, ran.stdout + ran.stderr);
}
