# Runtime examples — @ignaciano3/better-pdf

Each subdirectory is a standalone, runnable example for a specific JavaScript runtime.

## Support matrix

| Runtime | Entry file | Status | Notes |
|---------|-----------|--------|-------|
| [Node.js](./node/) | `index.mjs` | Verified (Node v24.16.0) | ESM, WASM self-initializes via `readFileSync` |
| [Bun](./bun/) | `index.ts` | Verified (Bun v1.3.14) | TypeScript, same Node entry condition |
| [Deno](./deno/) | `main.ts` | Config provided | `npm:` specifier, Deno not available in this env |
| [Vite](./vite/) | `src/main.ts` | Verified (build) — Vite v5.4.21 | `?url` import, wasm emitted as content-hashed asset |
| [webpack 5](./webpack/) | `src/index.js` | Verified (build) — webpack v5.107.2 | `new URL(…, import.meta.url)` + `asyncWebAssembly` |
| [Next.js](./nextjs/) | `app/page.tsx` | Verified (build) — Next.js v15.5.19 | `"use client"`, wasm served from `public/` via postinstall copy |
| [Cloudflare Workers](./cloudflare-workers/) | `src/index.ts` | Config provided | Browser entry + `import wasmModule` (CompiledWasm) — no `node:fs`, no runtime fetch |

**Verified** = example was installed and run in this environment; real output is shown in the subfolder README.  
**Config provided** = example source and configuration are complete; runtime was not available for local execution.

## Quick start

### Node.js

```sh
cd node
npm install @ignaciano3/better-pdf
node index.mjs
```

### Bun

```sh
cd bun
bun add @ignaciano3/better-pdf
bun index.ts
```

### Deno

```sh
cd deno
deno run -A main.ts
```

### Vite

```sh
cd vite
npm install
npm run dev     # dev server at http://localhost:5173
npm run build   # production build → vite/dist/
```

### webpack 5

```sh
cd webpack
npm install
npm run build   # production build → webpack/dist/
npx serve .     # serve index.html
```

### Next.js (App Router)

```sh
cd nextjs
npm install          # postinstall copies wasm to public/
npm run dev          # dev server at http://localhost:3000
npm run build        # production build → nextjs/.next/
```

### Cloudflare Workers

```sh
cd cloudflare-workers
npm install
npx wrangler dev     # local preview at http://localhost:8787
```

The worker returns `application/pdf`.  Deploy with `npx wrangler deploy`.
