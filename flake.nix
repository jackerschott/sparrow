{
    description = "A simple cli experiment submission tool compatible with slurm clusters";

    inputs =  {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    };

    outputs = { nixpkgs, ... }:
    let
        system = "x86_64-linux";
        pkgs = nixpkgs.legacyPackages.${system};
        sparrow = pkgs.rustPlatform.buildRustPackage {
            name = "sparrow";
            src = ./.;
            nativeBuildInputs = with pkgs; [
                pkg-config
                makeWrapper
            ];
            buildInputs = with pkgs; [
                openssl
                rsync
                fzf
            ];
            cargoHash = "sha256-Asbs7ynKkVJa9Dp9u4EHD4JHtGG/0fAdMgqX4cyhCq8=";
            postFixup = ''
                wrapProgram \
                    "$out/bin/sparrow" \
                    --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.rsync pkgs.fzf ]}
            '';
            meta = {
                description = "A simple cli experiment submission tool compatible with slurm clusters";
                homepage = "https://gitlab.cern.ch/jackersc/sparrow";
                license = pkgs.lib.licenses.mit;
            };
        };
    in {
        packages.${system}.default = sparrow;
    };
}
