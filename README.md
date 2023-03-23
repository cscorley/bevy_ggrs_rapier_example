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

### WASM

From the root directory:

```
cargo run --target wasm32-unknown-unknown
```

## Testing

- You can test rollbacks locally
  - On Linux, I use the included `slowmode.sh` script.
    - Run with root/sudo.
    - Run again to restore.
  - On Windows, I use clumsy https://jagt.github.io/clumsy/
