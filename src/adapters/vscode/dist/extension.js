"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const path = __importStar(require("node:path"));
const schema_1 = require("./schema");
const codegen_1 = require("./codegen");
const preview_1 = require("./preview");
const export_1 = require("./export");
const engine_1 = require("./engine");
class LpdfCodeLensProvider {
    provideCodeLenses(doc) {
        if (!(0, schema_1.isLpdfDocument)(doc)) {
            return [];
        }
        const idx = doc.getText().indexOf('<lpdf');
        if (idx === -1) {
            return [];
        }
        const pos = doc.positionAt(idx);
        const range = new vscode.Range(pos, pos);
        return [
            new vscode.CodeLens(range, { title: '▶ Preview PDF', command: 'lpdf.previewPdf' }),
            new vscode.CodeLens(range, { title: '⬇ Export PDF', command: 'lpdf.exportPdf' }),
        ];
    }
}
function activate(context) {
    const xsdPath = path.join(context.extensionPath, 'schema', 'lpdf.xsd');
    // Status bar
    const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBar.text = 'lpdf ◆';
    statusBar.tooltip = 'lpdf PDF document — click to preview';
    statusBar.command = 'lpdf.previewPdf';
    context.subscriptions.push(statusBar);
    function refreshStatusBar(editor) {
        if (editor && (0, schema_1.isLpdfDocument)(editor.document)) {
            statusBar.show();
        }
        else {
            statusBar.hide();
        }
    }
    // Commands
    context.subscriptions.push(vscode.commands.registerCommand('lpdf.previewPdf', (uri) => (0, preview_1.previewPdf)(context, uri)), vscode.commands.registerCommand('lpdf.exportPdf', (uri) => (0, export_1.exportPdf)(uri)));
    (0, codegen_1.registerCodegenCommands)(context);
    // CodeLens
    context.subscriptions.push(vscode.languages.registerCodeLensProvider({ language: 'xml' }, new LpdfCodeLensProvider()));
    // Schema association + status bar refresh
    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
        refreshStatusBar(editor);
        if (editor) {
            (0, schema_1.ensureSchemaAssociation)(editor.document, xsdPath);
            if ((0, schema_1.isLpdfDocument)(editor.document)) {
                (0, preview_1.renderForUri)(context, editor.document.uri, 'switch');
            }
        }
    }), vscode.workspace.onDidOpenTextDocument(doc => {
        (0, schema_1.ensureSchemaAssociation)(doc, xsdPath);
    }), vscode.workspace.onDidSaveTextDocument(doc => {
        if ((0, schema_1.isLpdfDocument)(doc)) {
            (0, preview_1.renderForUri)(context, doc.uri, 'save');
        }
    }));
    // Seed state for already-open editors
    for (const editor of vscode.window.visibleTextEditors) {
        (0, schema_1.ensureSchemaAssociation)(editor.document, xsdPath);
    }
    refreshStatusBar(vscode.window.activeTextEditor);
}
function deactivate() {
    (0, engine_1.disposeRenderWorker)();
}
//# sourceMappingURL=extension.js.map