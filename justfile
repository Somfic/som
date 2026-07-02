# Editor CLI used to install the VS Code extension.
# Override for VSCodium etc.:  just editor=codium extension-install
editor := "code"

# List available recipes.
default:
    @just --list

# Build the compiler (debug).
build:
    cargo build

# Run the test suite.
test:
    cargo test

# Install the `som` binary to ~/.cargo/bin (needed for the extension's LSP).
install-som:
    cargo install --path .

# Install the VS Code extension's JS dependencies.
[working-directory: 'editors/vscode']
extension-deps:
    bun install

# Compile the VS Code extension.
[working-directory: 'editors/vscode']
extension-build: extension-deps
    bun run compile

# Package the extension into a .vsix.
[working-directory: 'editors/vscode']
extension-package: extension-build
    bun run package

# Build, package, and install the extension into VS Code.
[working-directory: 'editors/vscode']
extension-install: extension-package
    {{editor}} --install-extension som-*.vsix --force
    @echo "Extension installed — reload VS Code (Ctrl+Shift+P -> Reload Window)."

# Full setup: install the som binary and the VS Code extension.
setup: install-som extension-install
