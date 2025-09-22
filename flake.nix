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
            src = pkgs.fetchFromGitHub {
                owner = "jackerschott";
                repo = "sparrow";
                rev = "cbc957662bec31cc5e2b1589fc55f5702a24cd81";
                sha256 = "sha256-+O79fzIFRjwxsRqglXEJ+8XIt4Eb4dXAz+/vvxkCIN4=";
            };
            nativeBuildInputs = with pkgs; [
                pkg-config
                makeWrapper
            ];
            buildInputs = with pkgs; [
                openssl
                rsync
                fzf
            ];
            cargoHash = "sha256-2HaqD8bIF4OvHfmFw5BVdwODnPNwBriBeo7Rx11C4ds=";
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
