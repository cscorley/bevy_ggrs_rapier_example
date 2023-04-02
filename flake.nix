{
  description = "Bevy GGRS Rapier Example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/22.11";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rusts = pkgs.rust-bin.stable.latest.complete.override {
          extensions = [ "rust-src" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in {
        devShells.default = with pkgs;
          mkShell.override { stdenv = clangStdenv; } rec {
            nativeBuildInputs = [ pkg-config ];

            buildInputs = [
              alsa-lib
              libxkbcommon
              udev
              vulkan-loader
              xorg.libX11
              xorg.libXcursor
              xorg.libXi
              xorg.libXrandr

              # debugger
              zlib
            ];
            packages = [
              mold
              rusts
              wasm-bindgen-cli
              binaryen
              simple-http-server
              nix-ld
            ];

            # Run-time paths for libs
            LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;

            # For running the debugger
            NIX_LD_LIBRARY_PATH =
              lib.makeLibraryPath ([ clangStdenv.cc.cc ] ++ buildInputs);
            # Requires impure flake
            # NIX_LD = lib.fileContents "${clangStdenv.cc}/nix-support/dynamic-linker";
            # Magic from discord
            NIX_LD = "${clangStdenv.cc.libc_bin}/bin/ld.so";

            RUST_BACKTRACE = 1;

            CARGO_BUILD_JOBS = 7;
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "clang";
            # TODO: requires nightly "-Zshare-generics=y"
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS =
              "-C link-arg=-fuse-ld=${lib.getExe mold}";
          };
      });
}
