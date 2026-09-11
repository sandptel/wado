{
  description = "wado: a headless Wayland compositor that streams itself over WebRTC";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    { self, nixpkgs, systems, ... }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);
      pkgsFor = nixpkgs.legacyPackages;
    in
    {
      # ── Packages ────────────────────────────────────────────────────────────
      packages = eachSystem (
        system:
        let
          pkgs = pkgsFor.${system};
          lib = pkgs.lib;

          # What the binary dlopens at runtime rather than links at build time: the GL/VA-API
          # stack is resolved by name when a session starts, so it has to be on the wrapped
          # binary's library path or the compositor fails only once someone connects.
          runtimeLibs = with pkgs; [
            libGL libglvnd mesa libgbm libdrm pixman
            wayland libxkbcommon libinput libdisplay-info
            vulkan-loader
            x264 ffmpeg_8
            systemd udev seatd dbus glib
          ];

          # target/ holds a 287 MB debug binary. Copying it into the store on every
          # evaluation is slow enough to look like a hang.
          src = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                base = baseNameOf (toString path);
              in
              !(type == "directory" && (base == "target" || base == ".git" || base == ".direnv"));
          };

          wado = pkgs.rustPlatform.buildRustPackage {
            pname = "wado";
            version = "0.0.2";
            inherit src;

            cargoLock = {
              lockFile = ./Cargo.lock;
              # Smithay tracks a git revision (pinned in the lockfile only, deliberately —
              # upstream `main` breaks API regularly). A git dependency has no crates.io
              # hash, so Nix needs one stated here.
              outputHashes = {
                "smithay-0.7.0" = "sha256-hclOFFKWY2hjVEQrE/whFuppf72JuwNoV2UwBk/pAh4=";
              };
            };

            # The workspace also holds `wado-client`, which targets wasm32 and is built with
            # `dx`, not cargo — asking for it here would build it for the host and fail.
            cargoBuildFlags = [ "-p" "wado" "-p" "wado-relay" ];

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook
              makeWrapper
              protobuf
            ];

            buildInputs = runtimeLibs;

            PROTOC = "${pkgs.protobuf}/bin/protoc";

            # Off deliberately. The test suite spawns processes and claims a Wayland socket,
            # which wants an XDG_RUNTIME_DIR the build sandbox does not have. Tests are run
            # with `cargo test` in the dev shell, where those exist.
            doCheck = false;

            postInstall = ''
              for bin in wado wado-relay; do
                wrapProgram $out/bin/$bin \
                  --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}" \
                  --set-default LIBVA_DRIVERS_PATH "${pkgs.mesa}/lib/dri"
              done
            '';

            meta = with lib; {
              description = "Headless Wayland compositor that streams its display over WebRTC";
              homepage = "https://github.com/sandptel/wado";
              license = licenses.agpl3Only;
              platforms = platforms.linux;
              mainProgram = "wado";
            };
          };
        in
        {
          inherit wado;
          default = wado;
        }
      );

      # `nix run github:sandptel/wado` starts the daemon; `#relay` starts the rendezvous.
      apps = eachSystem (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.wado}/bin/wado";
        };
        relay = {
          type = "app";
          program = "${self.packages.${system}.wado}/bin/wado-relay";
        };
      });

      # ── Development shell ───────────────────────────────────────────────────
      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor.${system};
          lib = pkgs.lib;

          # Rust compiler + cargo (toolchain comes from the shell, not rustup)
          rust = with pkgs; [
            cargo
            rustc
          ];

          # LLVM/clang libs — needed by clangStdenv and bindgen for FFI codegen
          llvm = with pkgs; [
            libllvm
            libclang
            llvmPackages.libclang
          ];

          # Font rendering for GUI windows / preview surfaces
          fonts = with pkgs; [
            freetype
            fontconfig
            noto-fonts
            noto-fonts-color-emoji
            dejavu_fonts
            freefont_ttf
          ];

          # Vulkan + OpenGL/Mesa stack, dlopen'd at runtime by the renderer
          graphics = with pkgs; [
            vulkan-loader
            vulkan-headers
            vulkan-validation-layers
            vulkan-tools
            shaderc
            libGL
            libglvnd
            mesa
            mesa-gl-headers
            libgbm
            libdrm
            glfw
            pixman
          ];

          # Wayland + X11 display servers and input handling
          display = with pkgs; [
            wayland
            wayland-protocols
            libxkbcommon
            libinput
            libdisplay-info
            libxcb
            libxcb-util
            libx11
            libxrandr
            libxi
            libxcursor
            libxxf86vm
          ];

          # GStreamer pipeline + audio + H.264 encode for video work
          media = with pkgs; [
            gst_all_1.gstreamer
            gst_all_1.gst-plugins-base
            gst_all_1.gst-plugins-good
            gst_all_1.gst-plugins-bad
            libpulseaudio
            x264
          ];

          # System services and misc shared libs pulled in at runtime
          systemLibs = with pkgs; [
            systemd
            udev
            seatd
            dbus
            glib
            gdk-pixbuf
            openssl
          ];

          # Make/ninja for C/C++ deps built from source
          buildLibs = with pkgs; [
            gnumake
            ninja
          ];

          # Everything the running binary links or dlopens — also fed to LD_LIBRARY_PATH
          runtimeLibs = rust ++ llvm ++ fonts ++ graphics ++ display ++ media ++ systemLibs ++ buildLibs;
        in
        {
          default = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {

            # Build-time tooling: linkers, codegen, compiler cache, Dioxus CLI
            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook
              mold
              lld
              protobuf
              sccache
              dbus
              pam
              # Pinned to the 8.x line: `ffmpeg-the-third = "5.0.0"` (crates/compositor) is
              # built against the FFmpeg 8.1 ABI. nixpkgs `ffmpeg` is 9.x now.
              ffmpeg_8 # video verification + libav* dev libs for the M3 FFI
              dioxus-cli
            ];

            buildInputs = runtimeLibs;

            # dlopen'd libs aren't on the default linker path; expose them here
            LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;

            # libva finds its driver by probing for __vaDriverInit_<VA major>_<minor>,
            # scanning DOWNWARD from its own version. A driver built against a NEWER libva
            # than ours is therefore invisible: it exports a number above where we start
            # counting. That is what broke VAAPI with -5 — the shell's libva was scanning
            # from 1_23 while the system driver (/run/opengl-driver) exported only 1_24.
            # Pointing libva at the shell's OWN mesa keeps the two in lockstep by
            # construction, so a NixOS rebuild that moves the system driver can't break us.
            LIBVA_DRIVERS_PATH = "${pkgs.mesa}/lib/dri";

            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            # Point bindgen's clang at the C headers it can't find on its own
            BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.glibc.dev}/include";

            shellHook = ''
              printf '\033[1;36m%s\033[0m\n' "wado › rust · clang · dioxus · x264/ffmpeg · vulkan/wayland · gstreamer"
              printf '\033[2m  env › PROTOC LIBCLANG_PATH RUST_SRC_PATH BINDGEN_EXTRA_CLANG_ARGS · linker: mold\033[0m\n'
            '';
          };
        }
      );
    };
}
