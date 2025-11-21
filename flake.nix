{
  description = "Generate a release note for your project";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
    git-hooks,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable."1.89.0".default.override {
          extensions = ["rust-src" "cargo" "rustc" "clippy" "rustfmt"];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        buildInputs = with pkgs; [
          alejandra
          cargo-insta
          nil
          openssl
          shellcheck
          shfmt
          typos
          zlib
        ];

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
        ];

        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          package = pkgs.prek;
          hooks = {
            claude-co-authored-by-trailer = {
              enable = true;
              name = "claude-co-authored-by-trailer";
              description = "Add Co-authored-by: Claude trailer to commits";
              entry = let
                script = pkgs.writeShellScript "claude-co-authored-by-trailer" ''
                  #!/usr/bin/env bash
                  # Add Co-Authored-By: Claude <noreply@anthropic.com> trailer to commit message
                  COMMIT_MSG_FILE="$1"
                  COMMIT_SOURCE="''${2:-}"

                  TRAILER="Co-Authored-By: Claude <noreply@anthropic.com>"

                  # Skip for merge/squash commits
                  if [ "$COMMIT_SOURCE" != "merge" ] && [ "$COMMIT_SOURCE" != "squash" ]; then
                    if [ -f "$COMMIT_MSG_FILE" ]; then
                      # Check if trailer already exists
                      if ! grep -q "$TRAILER" "$COMMIT_MSG_FILE"; then
                        echo "" >> "$COMMIT_MSG_FILE"
                        echo "$TRAILER" >> "$COMMIT_MSG_FILE"
                      fi
                    fi
                  fi
                '';
              in
                toString script;
              stages = ["prepare-commit-msg"];
            };
            typos = {
              enable = true;
              entry = "${pkgs.typos}/bin/typos";
            };
          };
        };
      in
        with pkgs; {
          checks = {
            inherit pre-commit-check;
          };

          devShells.default = mkShell {
            inherit nativeBuildInputs;
            inherit (pre-commit-check) shellHook;
            buildInputs = buildInputs ++ pre-commit-check.enabledPackages;
          };

          packages.default = pkgs.callPackage ./default.nix {
            inherit rustPlatform;
          };
        }
    );
}
