import * as vscode from 'vscode';
/**
 * Open or reveal the preview panel, then render the given URI.
 * Called by the "Preview PDF" command and the CodeLens button.
 * uri is passed when invoked from the Explorer context menu; falls back to the active editor.
 */
export declare function previewPdf(context: vscode.ExtensionContext, uri?: vscode.Uri): Promise<void>;
/**
 * Re-render the panel for a URI if the panel is already open.
 * reason 'switch': only re-renders if the URI differs from what is currently shown.
 * reason 'save':   only re-renders if the document content changed since the last render.
 */
export declare function renderForUri(context: vscode.ExtensionContext, uri: vscode.Uri, reason: 'switch' | 'save'): Promise<void>;
