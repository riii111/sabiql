{
  description = "sabiql development and build environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [
              "clippy"
              "rust-analyzer"
              "rustfmt"
            ];
          };
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };

          runtimePath = pkgs.lib.makeBinPath (
            [
              pkgs.graphviz
              pkgs.postgresql
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.xdg-utils ]
          );
        in
        {
          default = rustPlatform.buildRustPackage {
            pname = "sabiql";
            version = "1.11.0";

            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.makeWrapper
            ];

            postInstall = ''
              wrapProgram "$out/bin/sabiql" --prefix PATH : "${runtimePath}"
            '';

            meta = {
              description = "A fast, driver-less TUI for browsing and editing PostgreSQL databases";
              homepage = "https://github.com/riii111/sabiql";
              license = pkgs.lib.licenses.mit;
              mainProgram = "sabiql";
            };
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [
              "clippy"
              "rust-analyzer"
              "rustfmt"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              pkgs.cargo-audit
              pkgs.cargo-insta
              pkgs.cargo-nextest
              pkgs.graphviz
              pkgs.lefthook
              pkgs.postgresql
              pkgs.python3
              pkgs.ruby
            ];
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.nixfmt
      );
    };
}
