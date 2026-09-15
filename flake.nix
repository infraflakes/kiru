{
  description = "kiru development flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      crane,
      rust-overlay,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem =
        {
          self',
          system,
          ...
        }:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          lib = pkgs.lib;

          # The toolchain is pinned by `rust-toolchain.toml`, so the compiler
          # and every check use the same rustc, cargo, clippy, and rustfmt.
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # The distribution target is a static musl binary. The flake picks
          # the target per system and sets the two cargo variables from crane's
          # musl example, so the package, the checks, and the dev shell all
          # build it the same way. `rust-toolchain.toml` installs the std.
          staticTarget =
            {
              x86_64-linux = "x86_64-unknown-linux-musl";
              aarch64-linux = "aarch64-unknown-linux-musl";
            }
            .${system};
          targetEnv = {
            CARGO_BUILD_TARGET = staticTarget;
            CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          };

          # The compiler embeds `stdlib/io.kiru` with `include_str!`, so the
          # build source has to carry it even though cargo does not.
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              ./stdlib
              (lib.fileset.maybeMissing ./.cargo)
            ];
          };

          commonArgs = {
            inherit src;
            strictDeps = true;
          }
          // targetEnv;

          # Dependencies only need the cargo files, so their artifacts are not
          # rebuilt when `stdlib/` changes.
          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // { src = craneLib.cleanCargoSource ./.; });

          kiru = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              # Tests run as their own check, so a plain build does not repeat
              # them for downstream consumers.
              doCheck = false;
            }
          );
        in
        {
          packages.default = kiru;

          checks = {
            inherit kiru;

            kiru-clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );

            kiru-fmt = craneLib.cargoFmt { inherit src; };

            kiru-test = craneLib.cargoTest (
              commonArgs
              // {
                inherit cargoArtifacts;
              }
            );
          };

          devShells.default = craneLib.devShell (
            {
              checks = self'.checks;
              packages = with pkgs; [
                bun
                biome
                cargo-edit
              ];
            }
            // targetEnv
          );
        };
    };
}
