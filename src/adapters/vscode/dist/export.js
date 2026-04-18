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
exports.exportPdf = exportPdf;
const vscode = __importStar(require("vscode"));
const path = __importStar(require("node:path"));
const engine_1 = require("./engine");
async function exportPdf(uri) {
    const targetUri = uri ?? vscode.window.activeTextEditor?.document.uri;
    if (!targetUri) {
        return;
    }
    const doc = await vscode.workspace.openTextDocument(targetUri);
    if (doc.languageId !== 'xml') {
        vscode.window.showErrorMessage('lpdf: Active file is not an XML document.');
        return;
    }
    const licenseKey = vscode.workspace.getConfiguration('lpdf').get('licenseKey', '');
    const xmlDir = path.dirname(doc.uri.fsPath);
    const defaultName = path.basename(doc.uri.fsPath, '.xml') + '.pdf';
    const defaultUri = vscode.Uri.file(path.join(xmlDir, defaultName));
    const saveUri = await vscode.window.showSaveDialog({
        defaultUri,
        filters: { 'PDF': ['pdf'] },
    });
    if (!saveUri) {
        return;
    }
    const xml = doc.getText();
    try {
        const bytes = await (0, engine_1.renderPdf)(xml, licenseKey, xmlDir);
        await vscode.workspace.fs.writeFile(saveUri, bytes);
        vscode.window.showInformationMessage(`lpdf: Saved ${path.basename(saveUri.fsPath)}`);
    }
    catch (e) {
        const msg = e instanceof engine_1.LpdfRenderError ? e.message : String(e);
        vscode.window.showErrorMessage(`lpdf: ${msg}`);
    }
}
//# sourceMappingURL=export.js.map