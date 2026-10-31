{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        pythonEnv = pkgs.python3.withPackages (ps: [
            ps.scapy
            (ps.mpv or (ps."python-mpv" or ps.python_mpv))
            ps.stupidartnet
        ]);

        isLinux = pkgs.stdenv.isLinux;

        libraries = with pkgs; [
            libclang
            clang
            clangStdenv
            openssl
            mpv
        ] ++ pkgs.lib.optionals isLinux [
            udev
            wayland
            libxkbcommon
            fontconfig
            libGL
            alsa-lib
            xorg.libxcb
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            vulkan-loader
            pkgs.stdenv.cc.cc.lib
            glibc.dev
            gcc
            mold
            dbus
        ];

        packages = with pkgs; [
            pkg-config
            libclang
            clang
            clangStdenv
            openssl
            pkgs.binaryen
            mpv
            pythonEnv
        ] ++ pkgs.lib.optionals isLinux [
            libGL
            udev
            alsa-lib
            mold
            xorg.libxcb
            libxkbcommon
            wayland
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            vulkan-loader
            vulkan-tools
            pkgs.stdenv.cc.cc.lib
            dbus
            perf
        ];
      in
      {
        devShell = pkgs.mkShell {
          buildInputs = packages;

          shellHook =
            ''
              unset NIX_ENFORCE_PURITY
              if [ -d /run/opengl-driver ]; then
                export LD_LIBRARY_PATH=/run/opengl-driver/lib:/run/opengl-driver-32/lib:$LD_LIBRARY_PATH
                export XDG_DATA_DIRS=/run/opengl-driver/share:/run/opengl-driver-32/share:$XDG_DATA_DIRS
              fi
              export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libraries}:$LD_LIBRARY_PATH
              export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS

              # TODO: uncomment if we need dhat
              # cargo install dhat-to-flamegraph

              # if running from zsh, reenter zsh
              if [[ $(ps -e | grep $PPID) == *"zsh" ]]; then
                    export SHELL=zsh
                    zsh
                    exit
              fi
            '';
        };
      });
}
