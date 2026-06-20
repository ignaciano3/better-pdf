# Runtime examples — @ignaciano3/better-pdf

Each subdirectory is a standalone, runnable example for a specific JavaScript runtime.

## Support matrix

| Runtime | Entry file | Status | Notes |
|---------|-----------|--------|-------|
| [Node.js](./node/) | `index.mjs` | Verified (Node v24.16.0) | ESM, WASM self-initializes via `readFileSync` |
| [Bun](./bun/) | `index.ts` | Verified (Bun v1.3.14) | TypeScript, same Node entry condition |
| [Deno](./deno/) | `main.ts` | Config provided | `npm:` specifier, Deno not available in this env |
| Browser / bundlers | — | See Task 3 | — |
| Cloudflare Workers | — | See Task 4 | — |

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
