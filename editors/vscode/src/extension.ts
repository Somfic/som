import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  TransportKind,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

const MAIN_FN_REGEX = /^\s*fn\s+main\s*\(/;
const COMMENT_REGEX = /^\s*\/\//;

class SomCodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const lenses: vscode.CodeLens[] = [];
    for (let i = 0; i < document.lineCount; i++) {
      const line = document.lineAt(i).text;
      if (COMMENT_REGEX.test(line)) continue;
      if (MAIN_FN_REGEX.test(line)) {
        const range = new vscode.Range(i, 0, i, line.length);
        lenses.push(
          new vscode.CodeLens(range, {
            title: "$(play) Run",
            command: "som.run",
            arguments: [document.uri],
          })
        );
      }
    }
    return lenses;
  }
}

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("som");
  const serverPath = config.get<string>("lsp.path", "som-lsp");

  const serverOptions: ServerOptions = {
    command: serverPath,
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "som" }],
  };

  client = new LanguageClient("som", "Som Language Server", serverOptions, clientOptions);
  client.start();

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(
      { language: "som" },
      new SomCodeLensProvider()
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("som.run", (uri?: vscode.Uri) => {
      const fileUri = uri ?? vscode.window.activeTextEditor?.document.uri;
      if (!fileUri) {
        vscode.window.showErrorMessage("No active Som file to run.");
        return;
      }
      const projectDir = path.dirname(fileUri.fsPath);
      const compilerPath = vscode.workspace
        .getConfiguration("som")
        .get<string>("compiler.path", "som");

      let terminal = vscode.window.terminals.find((t) => t.name === "Som: Run");
      if (!terminal) {
        terminal = vscode.window.createTerminal("Som: Run");
      }
      terminal.show();
      terminal.sendText(`${compilerPath} ${projectDir}`);
    })
  );
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
