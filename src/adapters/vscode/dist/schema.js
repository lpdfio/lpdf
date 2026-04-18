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
exports.isLpdfDocument = isLpdfDocument;
exports.ensureSchemaAssociation = ensureSchemaAssociation;
const vscode = __importStar(require("vscode"));
function isLpdfDocument(doc) {
    if (doc.languageId !== 'xml')
        return false;
    return doc.getText().substring(0, 512).includes('<lpdf');
}
/**
 * Ensure a single workspace-scoped glob entry in xml.fileAssociations associates
 * all *.xml files in the workspace with the lpdf XSD.
 *
 * Replaces the old per-file global approach:
 *  - Written to ConfigurationTarget.Workspace so it lives in .vscode/settings.json,
 *    not the user's global settings.
 *  - Uses a glob pattern ("**\/*.xml") so it covers every lpdf file without
 *    accumulating per-file entries over time.
 *  - Idempotent: no-ops if an entry with this systemId already exists in any scope.
 *  - Cleans up any stale per-file entries the old implementation wrote to global config.
 */
async function ensureSchemaAssociation(doc, xsdPath) {
    if (!isLpdfDocument(doc))
        return;
    const systemId = vscode.Uri.file(xsdPath).toString();
    const config = vscode.workspace.getConfiguration('xml');
    // --- 1. Clean up stale per-file global entries written by the old implementation ---
    const global = config.inspect('fileAssociations');
    const globalEntries = global?.globalValue ?? [];
    const staleRemoved = globalEntries.filter(a => a.systemId !== systemId);
    if (staleRemoved.length !== globalEntries.length) {
        await config.update('fileAssociations', staleRemoved.length > 0 ? staleRemoved : undefined, vscode.ConfigurationTarget.Global);
    }
    // --- 2. Add one workspace-scoped glob entry if not already present ---
    const workspaceEntries = global?.workspaceValue ?? [];
    if (workspaceEntries.some(a => a.systemId === systemId))
        return;
    await config.update('fileAssociations', [...workspaceEntries, { systemId, pattern: '**/*.xml' }], vscode.ConfigurationTarget.Workspace);
}
//# sourceMappingURL=schema.js.map