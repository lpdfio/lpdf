<img src="https://raw.githubusercontent.com/lpdfio/lpdf/main/lpdf-mark.svg" height="48" alt="Lpdf" />

# [Lpdf](https://lpdf.io?utm_campaign=core&utm_medium=referral&utm_source=readme)

Lpdf is a PDF engine that renders a document written in XML or built in code to the same PDF in JavaScript, Python, Java, C# and PHP.

## Example

```xml
<lpdf version="1">
  <document size="letter">
    <section>
      <layout margin="48pt">
        <column gap="24pt">
          <row justify="between">
            <text font-size="8pt" color="#888888">ACME CORP</text>
            <text font-size="22pt" bold="true">Project Proposal</text>
          </row>
          <divider thickness="xs"/>
          <text font-size="13pt" bold="true">Scope of Work</text>
          <row gap="12pt" align="start">
            <text color="#888888" width="24pt">01</text>
            <text width="fill">Discovery &amp; Research</text>
          </row>
        </column>
      </layout>
    </section>
  </document>
</lpdf>
```

Every SDK renders it with `render`, and each SDK's README builds the same document in code.

## SDKs

### JavaScript

[github.com/lpdfio/lpdf-js](https://github.com/lpdfio/lpdf-js)  -  npm package [`@lpdfio/lpdf`](https://www.npmjs.com/package/@lpdfio/lpdf)  -  Node.js 20+

```bash
npm install @lpdfio/lpdf
```

### Python

[github.com/lpdfio/lpdf-python](https://github.com/lpdfio/lpdf-python)  -  PyPI package [`lpdfio-lpdf`](https://pypi.org/project/lpdfio-lpdf/)  -  Python 3.11+

```bash
pip install lpdfio-lpdf
```

### Java

[github.com/lpdfio/lpdf-java](https://github.com/lpdfio/lpdf-java)  -  Maven artifact [`io.lpdf:lpdf`](https://central.sonatype.com/artifact/io.lpdf/lpdf)  -  Java 17+

```xml
<dependency>
  <groupId>io.lpdf</groupId>
  <artifactId>lpdf</artifactId>
  <version>VERSION</version>
</dependency>
```

### C#

[github.com/lpdfio/lpdf-dotnet](https://github.com/lpdfio/lpdf-dotnet)  -  NuGet package [`Lpdfio.Lpdf`](https://www.nuget.org/packages/Lpdfio.Lpdf)  -  .NET 8+

```bash
dotnet add package Lpdfio.Lpdf
```

### PHP

[github.com/lpdfio/lpdf-php](https://github.com/lpdfio/lpdf-php)  -  Composer package [`lpdfio/lpdf`](https://packagist.org/packages/lpdfio/lpdf)  -  PHP 8.2+

```bash
composer require lpdfio/lpdf
```

## VS Code extension

[Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=lpdfio.lpdf)  -  [Open VSX](https://open-vsx.org/extension/lpdfio/lpdf)

A PDF viewer, a PDF preview and export for Markdown files, a visual diff for PDFs, and an editor for Lpdf's XML with a live preview.

## Docs

[lpdf.io/docs](https://lpdf.io/docs/?utm_campaign=core&utm_medium=referral&utm_source=readme)

## Issues

Report bugs and request features at [github.com/lpdfio/lpdf/issues](https://github.com/lpdfio/lpdf/issues), the one tracker for the engine, the SDKs and the VS Code extension. Questions go to [Discussions](https://github.com/lpdfio/lpdf/discussions). Pull requests are not accepted.

---

Licensed under the Lpdf License. See [LICENSE](LICENSE) for full terms.
