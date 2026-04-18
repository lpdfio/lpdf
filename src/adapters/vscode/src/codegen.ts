import * as vscode from 'vscode';

export function registerCodegenCommands(context: vscode.ExtensionContext): void {
  const stub = () => vscode.window.showInformationMessage('lpdf codegen coming soon.');
  context.subscriptions.push(
    vscode.commands.registerCommand('lpdf.generateTs',   stub),
    vscode.commands.registerCommand('lpdf.generateCs',   stub),
    vscode.commands.registerCommand('lpdf.generatePy',   stub),
    vscode.commands.registerCommand('lpdf.generatePhp',  stub),
    vscode.commands.registerCommand('lpdf.generateHere', stub),
  );
}
