{
  description = "Verdant — a quiet GPU-rendered ASCII wallpaper for Wayland";
  inputs = {
    nixpkgs.url = "https://github.com/NixOS/nixpkgs/archive/0bb2eedc20e4739f29e8045034f2828ad7054f8f.tar.gz";
    rust-overlay.url = "https://github.com/oxalica/rust-overlay/archive/6b29b8e8d3cc8ebda9342fbbcc7ac06eff58e2fb.tar.gz";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };
    in {
      devShells = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          default = pkgs.mkShell {
            packages = with pkgs; [ (rust-bin.stable.latest.default.override { extensions = [ "rust-src" ]; }) pkg-config wayland libglvnd ];
            LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath [ pkgs.wayland pkgs.libglvnd ]}:/run/opengl-driver/lib";
          };
        });
      packages = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          rustPlatform = pkgs.makeRustPlatform { cargo = pkgs.rust-bin.stable.latest.default; rustc = pkgs.rust-bin.stable.latest.default; };
        in rec {
          default = verdant;
          verdant = rustPlatform.buildRustPackage {
            pname = "verdant";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type: !(builtins.elem (baseNameOf path) [ "target" "previews" ".git" "result" "tree16x9.jpg" ]);
            };
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
            buildInputs = [ pkgs.wayland pkgs.libglvnd ];
            postFixup = ''
              wrapProgram $out/bin/verdant --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath [ pkgs.wayland pkgs.libglvnd ]}:/run/opengl-driver/lib
            '';
            meta = {
              description = "A GPU-rendered animated ASCII tree wallpaper for Wayland";
              homepage = "https://github.com/madprogrammer/verdant";
              mainProgram = "verdant";
              platforms = systems;
            };
          };
        });
      apps = forAllSystems (system: rec {
        default = verdant;
        verdant = {
          type = "app";
          program = "${self.packages.${system}.verdant}/bin/verdant";
          meta.description = self.packages.${system}.verdant.meta.description;
        };
      });
    };
}
