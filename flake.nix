{
  description = "TUI for configuring monitors on wlroots-based Wayland compositors";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "wlr-randr-tui";
        version = "0.1.0";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.ncurses ];
      };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [ rustc cargo pkg-config ncurses ];
      };
    };
}
