{
    description = "A simple cli experiment submission tool compatible with slurm clusters";

    inputs =  {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
        crane.url = "github:ipetkov/crane";
    };

    outputs = { nixpkgs, crane, ... }:
    let
        system = "x86_64-linux";
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;
        sparrowUnwrapped = craneLib.buildPackage {
            name = "sparrow";
            src = ./.;
            nativeBuildInputs = [
                pkgs.makeWrapper
                pkgs.pkg-config
                pkgs.openssl
                pkgs.openssl.dev
            ];

            #postFixup = ''
            #    wrapProgram \
            #        "$out/bin/sparrow" \
            #        --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.rsync ]}
            #'';
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };
        # this extra step is needed to add rsync to the PATH of sparrow,
        # since $out in craneLib.buildPackage is not the right path
        sparrow = pkgs.symlinkJoin {
            name = "sparrow";
            paths = [ sparrowUnwrapped ];
            buildInputs = [ pkgs.makeWrapper pkgs.rsync ];

            postBuild = ''
                wrapProgram \
                    "$out/bin/sparrow" \
                    --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.rsync ]}
            '';
            meta.mainProgram = "sparrow";
        };
    in {
        packages.${system}.default = sparrow;
    };
}
