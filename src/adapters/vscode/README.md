# lpdf for VS Code

**Layout-first PDF authoring, right inside your editor.**

lpdf lets you define PDF documents using a layout model — stacks, flanks, grids, frames — instead of placing elements at explicit coordinates. Author visually, bind live data, and convert your template to typed code in any supported language.

![lpdf demo](https://raw.githubusercontent.com/codesensedev/lpdf/master/src/adapters/vscode/media/demo.gif)

---

## Features

### Live PDF Preview
Open any lpdf XML file and trigger **lpdf: Preview PDF** to see a live PDF preview rendered via the WASM engine — entirely on your machine, no server required.

### Export PDF
**lpdf: Export PDF** renders the current XML template and writes the PDF to disk.

### XSD Autocomplete & Validation
The extension registers the lpdf XSD schema automatically. Open any `.lpdf.xml` file and get attribute autocomplete, element validation, and inline documentation — no configuration needed. Requires the [Red Hat XML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-xml).

### Visual Layout Editor
Use the **Open in lpdf Editor** CodeLens that appears at the top of any lpdf XML file. A layout preview panel opens beside the file. Edit visually — the XML stays in sync. Undo/redo spans both views. No separate app, no context switch.

### Codegen — TypeScript, C#, Python, PHP
Convert an XML template to typed builder code with a single command, inserted directly at your cursor:

| Command | Output |
|---|---|
| `lpdf: Generate TypeScript from XML` | TypeScript builder code |
| `lpdf: Generate C# from XML` | C# builder code |
| `lpdf: Generate Python from XML` | Python builder code |
| `lpdf: Generate PHP from XML` | PHP builder code |
| `lpdf: Generate lpdf code here` | Language chosen interactively |

When the template contains data-binding attributes (`data-source`, `data-value`, `data-if`), the generated code emits real data accessors — not placeholder strings.

### Data Binding Attributes

| Attribute | Effect |
|---|---|
| `data-value="path"` | Render a JSON path value as text. Inline text is the fallback. |
| `data-source="path"` | Repeat this element for each item in an array. |
| `data-if="path"` | Include this element only when the value is truthy. |
| `data-if-not="path"` | Include this element only when the value is falsy. |

Bind a JSON file to the template and the preview substitutes real values, expands loops, and respects conditionals in real time. Without a JSON file, all inline fallback values are shown, so the template always looks complete.

---

## Requirements

- [Red Hat XML](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-xml) — required for XSD autocomplete and validation (installed automatically as a dependency)
- A valid lpdf license key for export and codegen features (configure via `lpdf.licenseKey` in Settings)

---

## Extension Settings

| Setting | Description |
|---|---|
| `lpdf.licenseKey` | Your lpdf license key. Unlocks PDF export and code generation. |

---

## Fully Offline

The WASM renderer, visual editor, PDF preview, and codegen all run locally. No server round-trips, no accounts, no data leaves your machine. Critical for financial, HR, legal, and compliance documents.

---

## Get a License

Visit [lpdf.io/pricing](https://lpdf.io/pricing) to purchase a license. Once you have a key, set it in VS Code Settings under `lpdf.licenseKey`.

---

## More Information

- [Documentation](https://lpdf.io/docs)
- [GitHub](https://github.com/lpdf-io/lpdf)
- [lpdf.io](https://lpdf.io)
