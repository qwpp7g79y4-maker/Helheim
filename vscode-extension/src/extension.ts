import * as vscode from 'vscode';

export function activate(context: vscode.ExtensionContext) {
  console.log('[Helheim] Language extension activated');
  // Hier kun je later hover providers, diagnostics, etc. registreren.
}

export function deactivate() {
  console.log('[Helheim] Language extension deactivated');
}
