{
  description = "Development environment for the Som programming language";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };
      in {
        # som reads C headers with `cc -Xclang -ast-dump=json`, so `cc` must be clang.
        devShells.default = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            cargo-llvm-cov
            just
            cargo-edit
            cargo-watch
            cargo-deny
            llvm
            ripgrep
            fd
            bat
            raylib
            box2d
            sqlite
            sdl3
            wgpu-native
          ];

          shellHook = ''
            export PATH="$HOME/.cargo/bin:$PATH"

            # Install committer if not already installed
            if ! command -v committer &> /dev/null; then
              echo "Installing committer..."
              cargo install committer
            fi
          '';

          # libs/wgpu includes `webgpu.h` bare, as Homebrew installs it; Nix keeps it in include/webgpu.
          CPATH = "${pkgs.wgpu-native.dev}/include/webgpu";
          # wgpu loads Vulkan at runtime rather than linking it.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader ];

          RUST_BACKTRACE = "1";
          RUST_LOG = "debug";
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
