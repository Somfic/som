import * as vscode from "vscode";
import {
  LanguageClient,
  TransportKind,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function somPath(): string {
  return vscode.workspace.getConfiguration("som").get<string>("path", "som");
}

function startClient(): LanguageClient {
  // The language server is the som binary itself, in `lsp` mode.
  const serverOptions: ServerOptions = {
    command: somPath(),
    args: ["lsp"],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "som" }],
  };

  const c = new LanguageClient("som", "Som Language Server", serverOptions, clientOptions);
  c.start();
  return c;
}

export function activate(context: vscode.ExtensionContext) {
  client = startClient();

  context.subscriptions.push(
    vscode.commands.registerCommand("som.restartServer", async () => {
      await client?.stop();
      client = startClient();
    }),

    vscode.commands.registerCommand("som.run", (arg?: vscode.Uri | string) => {
      const fileUri =
        (typeof arg === "string" ? vscode.Uri.parse(arg) : arg) ??
        vscode.window.activeTextEditor?.document.uri;
      if (!fileUri) {
        vscode.window.showErrorMessage("No active Som file to run.");
        return;
      }

      let terminal = vscode.window.terminals.find((t) => t.name === "Som: Run");
      if (!terminal) {
        terminal = vscode.window.createTerminal("Som: Run");
      }
      terminal.show();
      terminal.sendText(`${somPath()} run ${JSON.stringify(fileUri.fsPath)}`);
    })
  );
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
