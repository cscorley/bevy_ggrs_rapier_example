{ pkgs ? import <nixpkgs> {
  overlays = [
    (import (builtins.fetchTarball
      "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"))
  ];
} }:

let
  rusts = pkgs.rust-bin.stable.latest.complete.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  };
in pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } rec {
  name = "bevy_ggrs_rapier_example";
  nativeBuildInputs = (with pkgs; [ pkg-config ]);
  buildInputs = (with pkgs; [
    # bevy deps
    udev
    alsa-lib
    vulkan-loader
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    libxkbcommon
    # wayland
  ]);

  packages = [ rusts ] ++ (with pkgs; [
    mold

    # wasm
    wasm-bindgen-cli
    binaryen
    simple-http-server
  ]);

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
  RUST_BACKTRACE = 1;

  # Leave myself 1 core free :-)
  CARGO_BUILD_JOBS = "7";
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "clang";
  # TODO: requires nightly "-Zshare-generics=y"
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS =
    "-C link-arg=-fuse-ld=${pkgs.lib.getExe pkgs.mold}";

}
