import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

/**
 * The `som` binary, which runs programs (`som run`) and is the language server (`som lsp`):
 * the `som.path` setting when it's set; otherwise a build in the open workspace's
 * `target/` (the newest of debug and release), so working on som itself uses what was
 * just built; otherwise `som` from PATH.
 */
function som(): string {
  const configured = vscode.workspace.getConfiguration("som").get<string>("path", "").trim();
  if (configured) {
    return configured;
  }

  const file = process.platform === "win32" ? "som.exe" : "som";
  const builds = (vscode.workspace.workspaceFolders ?? [])
    .flatMap((folder) => ["debug", "release"].map((profile) => path.join(folder.uri.fsPath, "target", profile, file)))
    .filter((candidate) => fs.existsSync(candidate))
    .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
  return builds[0] ?? "som";
}

/**
 * What to run for a file: its directory when that's a multi-file program (it has a
 * `main.som`), otherwise just the file.
 */
function programFor(file: string): string {
  const dir = path.dirname(file);
  return fs.existsSync(path.join(dir, "main.som")) ? dir : file;
}

async function startClient(): Promise<LanguageClient | undefined> {
  const command = som();
  // stdin/stdout is the default transport; naming it would make the client add `--stdio`.
  const serverOptions: ServerOptions = { command, args: ["lsp"] };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "som" }],
  };

  const c = new LanguageClient("som", "Som Language Server", serverOptions, clientOptions);
  try {
    await c.start();
    return c;
  } catch (err) {
    vscode.window.showErrorMessage(
      `Som: could not start the language server \`${command} lsp\`. ` +
        "Install som with `cargo install --path crates/tools/cli`, or set `som.path`. " +
        `(${err instanceof Error ? err.message : String(err)})`
    );
    return undefined;
  }
}

/** Compiles and runs a program as a task, so its output and exit code show in the terminal. */
async function run(fileUri: vscode.Uri) {
  await vscode.workspace.saveAll(false);

  const program = programFor(fileUri.fsPath);
  const binary = som();
  const task = new vscode.Task(
    { type: "som", command: "run", program },
    vscode.workspace.getWorkspaceFolder(fileUri) ?? vscode.TaskScope.Workspace,
    `run ${path.basename(program)}`,
    "som",
    // Run the binary directly rather than through a shell: no quoting, and nothing gets
    // typed into a program that's still running.
    new vscode.ProcessExecution(binary, ["run", program], { cwd: path.dirname(program) })
  );
  task.presentationOptions = {
    reveal: vscode.TaskRevealKind.Always,
    panel: vscode.TaskPanelKind.Dedicated,
    clear: true,
    showReuseMessage: false,
  };

  try {
    await vscode.tasks.executeTask(task);
  } catch (err) {
    vscode.window.showErrorMessage(
      `Som: could not run \`${binary}\`. ` +
        "Install it with `cargo install --path crates/tools/cli`, or set `som.path`. " +
        `(${err instanceof Error ? err.message : String(err)})`
    );
  }
}

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("som.restartServer", async () => {
      await client?.stop();
      client = await startClient();
    }),

    vscode.commands.registerCommand("som.run", async (arg?: vscode.Uri | string) => {
      const fileUri =
        (typeof arg === "string" ? vscode.Uri.parse(arg) : arg) ??
        vscode.window.activeTextEditor?.document.uri;
      if (!fileUri) {
        vscode.window.showErrorMessage("No active Som file to run.");
        return;
      }
      await run(fileUri);
    })
  );

  client = await startClient();
}

export function deactivate(): Promise<void> | undefined {
  return client?.stop();
}
