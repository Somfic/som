# Som for VS Code

Language support for [Som](../../readme.md): syntax highlighting, live
diagnostics, semantic-token highlighting, and a Run command.

The language server is the `som` binary itself (`som lsp`), so there is no
separate server to install — just build the compiler and point the extension at
it.

## Setup

1. Build and install the `som` binary so it's on your `PATH`:

   ```sh
   cargo install --path .        # from the repo root
   ```

   Or set `som.path` in your VS Code settings to an absolute path, e.g.
   `/home/you/som/target/debug/som`.

2. Build the extension (the dev shell provides `bun`):

   ```sh
   cd editors/vscode
   bun install
   bun run compile
   ```

3. Install it, one of two ways:

   - **Try it quickly** — open the `editors/vscode` folder in VS Code and press
     `F5`. This opens an Extension Development Host window with the extension
     loaded; open any `.som` file there.

   - **Install it for real** — package a `.vsix` and install it into your
     normal VS Code:

     ```sh
     bun run package                       # produces som-0.1.0.vsix
     code --install-extension som-0.1.0.vsix
     ```

     (Use `codium` instead of `code` for VSCodium.) Reload the window
     afterwards. Re-run these two commands to update after code changes.

Open any `.som` file and you should get highlighting and red squiggles on
errors as you type. Use **Som: Run** (the ▶ in the editor title bar) to run the
current file, and **Som: Restart Language Server** after rebuilding `som`.

## What the server provides

- **Diagnostics** — every open file is check-compiled (parse → type-check →
  MIR, no codegen) on each change and errors/warnings are published with their
  spans.
- **Semantic tokens** — keyword/type/number/string/comment/operator/identifier
  highlighting derived from the lexer.

Hover, go-to-definition, and completion aren't implemented yet — they need
name/type introspection the compiler doesn't expose. The server lives in
[`crates/lsp`](../../crates/lsp).

## Settings

| Setting    | Default | Description                                              |
| ---------- | ------- | -------------------------------------------------------- |
| `som.path` | `som`   | Path to the `som` binary (used for `som lsp` and `som run`). |
