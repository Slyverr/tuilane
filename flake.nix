{
  description = "tuilane, a local-first terminal workspace";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          tuilane = pkgs.callPackage ./nix/package.nix { };
        in
        {
          default = tuilane;
          inherit tuilane;
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.tuilane;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/tuilane";
            meta.description = "Run tuilane";
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.callPackage ./nix/dev-shell.nix { };
          runtime-evaluation = pkgs.callPackage ./nix/runtime-evaluation-shell.nix { };
        }
      );

      formatter = forAllSystems (system: (import nixpkgs { inherit system; }).nixfmt);

      checks = forAllSystems (system: {
        package = self.packages.${system}.tuilane;
      });
    };
}
