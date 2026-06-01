{
  description = "kiru development flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem =
        {
          config,
          self',
          inputs',
          pkgs,
          system,
          ...
        }:
        {
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "kiru";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter =
                path: type:
                let
                  baseName = builtins.baseNameOf path;
                in
                !(builtins.elem baseName [
                  "target"
                  ".git"
                  ".direnv"
                ]);
            };

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs = [ pkgs.installShellFiles ];

            postInstall = ''
              installShellCompletion --cmd kiru \
                --bash completions/kiru.bash \
                --fish completions/kiru.fish \
                --zsh completions/_kiru
            '';
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ config.packages.default ];
            buildInputs = with pkgs; [
              cargo
              clippy
              rustfmt
              cargo-edit
            ];
          };
        };
    };
}
