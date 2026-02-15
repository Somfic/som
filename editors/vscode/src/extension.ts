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
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
