import * as vscode from 'vscode';
import * as path from 'node:path';
import { isLpdfDocument, ensureSchemaAssociation } from './schema';
import { registerCodegenCommands } from './codegen';
import { previewPdf, renderForUri } from './preview';
import { exportPdf } from './export';
import { disposeRenderWorker } from './engine';

class LpdfCodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(doc: vscode.TextDocument): vscode.CodeLens[] {
    if (!isLpdfDocument(doc)) { return []; }
    const idx = doc.getText().indexOf('<lpdf');
    if (idx === -1) { return []; }
    const pos = doc.positionAt(idx);
    const range = new vscode.Range(pos, pos);
    return [
      new vscode.CodeLens(range, { title: '▶ Preview PDF', command: 'lpdf.previewPdf' }),
      new vscode.CodeLens(range, { title: '⬇ Export PDF',  command: 'lpdf.exportPdf'  }),
    ];
  }
}

export function activate(context: vscode.ExtensionContext): void {
  const xsdPath = path.join(context.extensionPath, 'schema', 'lpdf.xsd');

  // Status bar
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusBar.text = 'lpdf ◆';
  statusBar.tooltip = 'lpdf PDF document — click to preview';
  statusBar.command = 'lpdf.previewPdf';
  context.subscriptions.push(statusBar);

  function refreshStatusBar(editor?: vscode.TextEditor): void {
    if (editor && isLpdfDocument(editor.document)) {
      statusBar.show();
    } else {
      statusBar.hide();
    }
  }

  // Commands
  context.subscriptions.push(
    vscode.commands.registerCommand('lpdf.previewPdf', (uri?: vscode.Uri) => previewPdf(context, uri)),
    vscode.commands.registerCommand('lpdf.exportPdf',  (uri?: vscode.Uri) => exportPdf(uri)),
  );
  registerCodegenCommands(context);

  // CodeLens
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider({ language: 'xml' }, new LpdfCodeLensProvider()),
  );

  // Schema association + status bar refresh
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(editor => {
      refreshStatusBar(editor);
      if (editor) {
        ensureSchemaAssociation(editor.document, xsdPath);
        if (isLpdfDocument(editor.document)) {
          renderForUri(context, editor.document.uri, 'switch');
        }
      }
    }),
    vscode.workspace.onDidOpenTextDocument(doc => {
      ensureSchemaAssociation(doc, xsdPath);
    }),
    vscode.workspace.onDidSaveTextDocument(doc => {
      if (isLpdfDocument(doc)) { renderForUri(context, doc.uri, 'save'); }
    }),
  );

  // Seed state for already-open editors
  for (const editor of vscode.window.visibleTextEditors) {
    ensureSchemaAssociation(editor.document, xsdPath);
  }
  refreshStatusBar(vscode.window.activeTextEditor);
}

export function deactivate(): void {
  disposeRenderWorker();
}
