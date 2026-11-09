# Web build

The browser build is the same Rust app compiled to `wasm32-unknown-unknown`
and bundled by [Trunk](https://trunk-rs.github.io/trunk/).

## Tools

- Rust with the `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
- Trunk 0.22.1 (matches the pinned `wasm-bindgen` 0.2.130 in `Cargo.toml`)
- No Node.js toolchain is required.

## Build and check

```sh
scripts/test-web.sh
```

The script selects the active rustup toolchain (so a system `/usr/bin/cargo`
without the wasm target cannot be picked up), adds the target, runs
`cargo check --target wasm32-unknown-unknown`, then `trunk build --release` and
asserts that `dist/` contains the HTML, wasm, and JS artifacts.

## Run locally

```sh
trunk serve --release
```

Then open <http://127.0.0.1:8080/>. Release assets land in `dist/` (ignored by
git). WebGPU availability is browser-specific; see the ticket 54 gate. This
short guide is intentionally minimal — ticket 09 owns the final README.
