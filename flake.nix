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
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
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

        buildInputs = with pkgs; [
          alejandra
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
      in
        with pkgs; {
          devShells.default = mkShell {
            inherit buildInputs nativeBuildInputs;
          };
        }
    );
}
