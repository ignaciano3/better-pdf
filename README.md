# better-pdf

A maintained, fast alternative to pdf-lib for filling and flattening PDF AcroForms.
Runs in the browser and on the server via a Rust core compiled to WebAssembly.

> Status: pre-alpha. Milestone 1 (WASM round-trip pipeline) only.

## Develop

Prerequisites: `bun`, the Rust toolchain, the wasm32 target, and `wasm-pack`.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
bun install
bun run build:wasm   # compile the Rust core to WASM (writes pkg/)
bun test             # run the test suite
```
