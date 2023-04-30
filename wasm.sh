#!/usr/bin/env bash

set -e

mkdir -p wasm
cargo build --target wasm32-unknown-unknown --profile wasm-release --features web
wasm-bindgen --no-typescript --out-name bevy_ggrs_rapier_example --out-dir wasm --target web target/wasm32-unknown-unknown/wasm-release/bevy_ggrs_rapier_example.wasm
cp index.html wasm/

# TODO: if you have assets, copy them over!
# cp -r assets wasm/

# re-optimize with wasm stuff
# this part is very slow, maybe only run in CI?
wasm-opt -Oz --output wasm/optimized.wasm wasm/bevy_ggrs_rapier_example_bg.wasm
mv wasm/optimized.wasm wasm/bevy_ggrs_rapier_example_bg.wasm

# also can use cargo install basic-http-server
# only listing 127.0.0.1 here because I'm a doofus that clicks the link the
# program outputs, which is by default 0.0.0.0.  this address does not work for
# webrtc in firefox, you should visit 127!
simple-http-server wasm --nocache --ip 127.0.0.1
