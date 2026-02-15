import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  TransportKind,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

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
    vscode.commands.registerCommand("som.run", (arg?: vscode.Uri | string) => {
      const fileUri = (typeof arg === "string" ? vscode.Uri.parse(arg) : arg)
        ?? vscode.window.activeTextEditor?.document.uri;
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
