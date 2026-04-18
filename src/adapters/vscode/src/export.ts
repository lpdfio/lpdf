import * as vscode from 'vscode';
import * as path from 'node:path';
import { renderPdf, LpdfRenderError } from './engine';

export async function exportPdf(uri?: vscode.Uri): Promise<void> {
  const targetUri = uri ?? vscode.window.activeTextEditor?.document.uri;
  if (!targetUri) { return; }

  const doc = await vscode.workspace.openTextDocument(targetUri);
  if (doc.languageId !== 'xml') {
    vscode.window.showErrorMessage('lpdf: Active file is not an XML document.');
    return;
  }

  const licenseKey = vscode.workspace.getConfiguration('lpdf').get<string>('licenseKey', '');

  const xmlDir     = path.dirname(doc.uri.fsPath);
  const defaultName = path.basename(doc.uri.fsPath, '.xml') + '.pdf';
  const defaultUri  = vscode.Uri.file(path.join(xmlDir, defaultName));

  const saveUri = await vscode.window.showSaveDialog({
    defaultUri,
    filters: { 'PDF': ['pdf'] },
  });
  if (!saveUri) { return; }

  const xml = doc.getText();
  try {
    const bytes = await renderPdf(xml, licenseKey, xmlDir);
    await vscode.workspace.fs.writeFile(saveUri, bytes);
    vscode.window.showInformationMessage(`lpdf: Saved ${path.basename(saveUri.fsPath)}`);
  } catch (e) {
    const msg = e instanceof LpdfRenderError ? e.message : String(e);
    vscode.window.showErrorMessage(`lpdf: ${msg}`);
  }
}
