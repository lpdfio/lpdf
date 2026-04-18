import * as vscode from 'vscode';

interface XmlFileAssociation {
  systemId: string;
  pattern: string;
}

export function isLpdfDocument(doc: vscode.TextDocument): boolean {
  if (doc.languageId !== 'xml') return false;
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
export async function ensureSchemaAssociation(doc: vscode.TextDocument, xsdPath: string): Promise<void> {
  if (!isLpdfDocument(doc)) return;

  const systemId = vscode.Uri.file(xsdPath).toString();
  const config   = vscode.workspace.getConfiguration('xml');

  // --- 1. Clean up stale per-file global entries written by the old implementation ---
  const global = config.inspect<XmlFileAssociation[]>('fileAssociations');
  const globalEntries = global?.globalValue ?? [];
  const staleRemoved  = globalEntries.filter(a => a.systemId !== systemId);
  if (staleRemoved.length !== globalEntries.length) {
    await config.update(
      'fileAssociations',
      staleRemoved.length > 0 ? staleRemoved : undefined,
      vscode.ConfigurationTarget.Global,
    );
  }

  // --- 2. Add one workspace-scoped glob entry if not already present ---
  const workspaceEntries = global?.workspaceValue ?? [];
  if (workspaceEntries.some(a => a.systemId === systemId)) return;

  await config.update(
    'fileAssociations',
    [...workspaceEntries, { systemId, pattern: '**/*.xml' }],
    vscode.ConfigurationTarget.Workspace,
  );
}
