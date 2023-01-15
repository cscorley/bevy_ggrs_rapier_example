#!/usr/bin/env bash

set -e

mkdir -p wasm
cargo build --target wasm32-unknown-unknown --profile wasm-release --features web
wasm-bindgen --no-typescript --out-name bevy_ggrs_rapier_example --out-dir wasm --target web target/wasm32-unknown-unknown/wasm-release/bevy_ggrs_rapier_example.wasm
cp -r index.html wasm/

# TODO: if you have assets, copy them over!
# cp -r assets wasm/

# re-optimize with wasm stuff
# this part is very slow, maybe only run in CI?
wasm-opt -Oz --output wasm/optimized.wasm wasm/bevy_ggrs_rapier_example_bg.wasm
mv wasm/optimized.wasm wasm/bevy_ggrs_rapier_example_bg.wasm

# also can use cargo install basic-http-server
simple-http-server wasm --nocache
