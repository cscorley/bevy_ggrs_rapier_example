# Bevy GGRS Rapier example

This is my quest to get GGRS and Rapier to work together in the Bevy engine,
using the plugins.

Is it perfect? No. But is well-written? Also no.

Hopefully this will serve as a crash course to getting your fun weekend project
going.

Things I have going

- Deterministic physics and rollbacks (allegedly)
- Desync detection (1v1 only)
- Plenty poorly strung-together comments
- And a whole lot of debug learning

Keys

- WASD movement
- R turn on random movement for this window
- T turn off random movement for this window

## Building

### Native

From the root directory:

```
cargo run
```

In VS Code, you can also run
[two of them](https://www.youtube.com/watch?v=btHpHjabRcc) with
(Ctrl|Cmd)+Shift+B

### WASM

From the root directory (requires wasm-server-runner):

```
cargo run --target wasm32-unknown-unknown --features web
```

Or, use the `wasm.sh` script which runs several commands. This will produce an
optimized WASM build and launch a test HTTP server (requires wasm-bindgen-cli,
binaryen, and simple-http-server)

## Testing

- You can test rollbacks locally
  - On Linux, I use the included `slowmode.sh` script.
    - Run with root/sudo.
    - Run again to restore.
  - On Windows, I use clumsy https://jagt.github.io/clumsy/

# Contributing

Please do! Pull requests are always welcome; and don't be afraid to checkout the
[Bevy discord](https://discord.gg/bevy) for more help.
