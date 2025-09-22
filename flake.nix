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

        bwtoolsPackage = pkgs.rustPlatform.buildRustPackage {
          pname = "bwtools";
          version = "0.2.9";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "chrome-cache-parser-0.2.3" = "sha256-fpgAV26pmde6ETdcNPwkdfwS0aJKLXtzcRStPYll07g=";
            };
          };

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          postInstall = ''
            bundle="$out/bwtools"
            mkdir -p "$bundle"

            # Move the compiled binary into the bundle root so runtime assets live alongside it.
            mv "$out/bin/bwtools" "$bundle/bwtools"

            # Ship the bundled resources expected at runtime.
            cp "${./player_list.json}" "$bundle/player_list.json"
            mkdir -p "$bundle/overlay" "$bundle/history" "$bundle/.meta"

            # Provide a thin wrapper on PATH that executes the bundled binary.
            cat > "$out/bin/bwtools" <<EOF
#!${pkgs.runtimeShell}
exec "$bundle/bwtools" "\$@"
EOF
            chmod +x "$out/bin/bwtools"
          '';
        };
      in
      {
        packages = {
          bwtools = bwtoolsPackage;
          default = bwtoolsPackage;
        };

        devShells.default = with pkgs; mkShell {
          buildInputs = [
            openssl
            pkg-config
            rust-bin.stable.latest.default
          ];
        };
      }
    );
}
