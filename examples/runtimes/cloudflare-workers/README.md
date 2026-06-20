# better-pdf — Cloudflare Workers example

Creates a one-page PDF with drawn text and returns it as an `application/pdf` response.

> **Config provided** — `wrangler` is not installed in the development environment where this was authored.  The example source and configuration are complete and correct; they have not been executed here.  Verify by running in an environment with wrangler installed (see below).

---

## Why this workaround is needed

Cloudflare Workers impose two constraints that break the default package entry:

1. **No `node:fs`.**  The default entry (`@ignaciano3/better-pdf`) auto-initialises the WASM binary via `readFileSync`, which requires `node:fs`.  Workers have no `node:fs`, so the default entry throws at startup.

2. **No runtime WASM fetch.**  Workers cannot fetch arbitrary binaries at runtime without bundling them first.  The standard "fetch the .wasm URL" pattern therefore also fails.

**Solution:** import the `/browser` entry (no `node:fs`, explicit `initializeWasm()`) and let wrangler compile the `.wasm` file into a `WebAssembly.Module` binding at bundle time:

```ts
import { PdfDocument, initializeWasm, StandardFonts } from "@ignaciano3/better-pdf/browser";
import wasmModule from "@ignaciano3/better-pdf/wasm"; // wrangler → WebAssembly.Module
```

The `[[rules]]` block in `wrangler.toml` tells wrangler to apply the `CompiledWasm` type to all `*.wasm` imports, which is what turns the raw `.wasm` file into a `WebAssembly.Module` that can be passed directly to `initializeWasm()`.

---

## Install

```sh
cd examples/runtimes/cloudflare-workers
npm install
```

## Dev server (local preview)

```sh
npx wrangler dev
```

Then open the URL printed by wrangler (usually `http://localhost:8787`). Your browser should prompt you to download a PDF, or you can confirm the content type with:

```sh
curl -s -o /dev/null -w "%{content_type}\n" http://localhost:8787
# expected: application/pdf
```

### Expected behaviour

The worker returns an HTTP 200 response with:

- `Content-Type: application/pdf`
- Body: a valid PDF containing the text "hello from a worker"

## Deploy

```sh
npx wrangler deploy
```

You will need a Cloudflare account and to be logged in (`npx wrangler login`).

---

## How it works

| Step | Detail |
|------|--------|
| `import … from "@ignaciano3/better-pdf/browser"` | Uses the browser-targeted entry — no `node:fs` dependency |
| `import wasmModule from "@ignaciano3/better-pdf/wasm"` | The `./wasm` subpath export resolves to the raw `.wasm` file |
| `[[rules]] type = "CompiledWasm"` | wrangler compiles the `.wasm` into a `WebAssembly.Module` at bundle time |
| `await initializeWasm(wasmModule)` | Passes the pre-compiled Module directly — no fetch, no filesystem access |
| `doc.save()` | Returns a `Uint8Array` of PDF bytes sent as the response body |
