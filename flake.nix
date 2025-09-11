{
  description = "A devShell example";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShells.default = with pkgs; mkShell {
          buildInputs = [
            openssl
            pkg-config
            rust-bin.stable.latest.default
          ];
        };

        # Cross-compile to Windows (x86_64-pc-windows-gnu) on NixOS
        devShells.windows = with pkgs; mkShell {
          buildInputs = [
            pkg-config
            # Rust toolchain with Windows GNU target enabled
            (rust-bin.stable.latest.default.override {
              targets = [ "x86_64-pc-windows-gnu" ];
            })
            # MinGW toolchain from pkgsCross
            pkgsCross.mingwW64.stdenv.cc
          ];

          # Tell cargo to build for Windows by default in this shell
          CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
          # Linker and archiver for the Windows target
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${pkgsCross.mingwW64.stdenv.cc.targetPrefix}gcc";
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR = "${pkgsCross.mingwW64.stdenv.cc.targetPrefix}ar";
        };
      }
    );
}
