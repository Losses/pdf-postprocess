{
  description = "A rust devShell";

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
      with pkgs;
      {
        devShells.default = mkShell {
          buildInputs = [
            openssl
            pkg-config
            eza
            fd
            cairo
            librsvg

            # Use rust package from rust-overlay
            (rust-bin.beta.latest.default.override {
              extensions = [ "rust-src" ];
            })
          ];

          shellHook = ''
            alias ls=exa
            alias find=fd
            export RUST_BACKTRACE=1
          '';
        };
      }
    );
}