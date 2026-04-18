import * as vscode from 'vscode';
export declare function isLpdfDocument(doc: vscode.TextDocument): boolean;
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
export declare function ensureSchemaAssociation(doc: vscode.TextDocument, xsdPath: string): Promise<void>;
