# better-pdf — Node.js example

Creates a one-page PDF with drawn text and writes it to `out.pdf`.

## Install

```sh
npm install @ignaciano3/better-pdf
```

## Run

```sh
node index.mjs
```

### Expected output

```
Bytes written : 829
PDF header    : %PDF-
Starts with %%PDF- : true
Saved to out.pdf
```

## Verified: Node v24.16.0

```
$ node index.mjs
Bytes written : 829
PDF header    : %PDF-
Starts with %%PDF- : true
Saved to out.pdf
```

Run on 2026-06-20 using `@ignaciano3/better-pdf@0.20.0` (installed from local tarball; end users `npm install @ignaciano3/better-pdf`).

## How it works

The package default export (`dist/index.js`) initializes the WASM binary via `readFileSync` at import time — no explicit `initializeWasm()` call needed in Node.
