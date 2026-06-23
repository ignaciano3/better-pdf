---
title: Examples overview
description: Copy-paste task recipes and runnable per-runtime starter projects for better-pdf.
---

Two kinds of example live here:

- **Task recipes** — short, self-contained snippets you can paste into any
  project. Each does one end-to-end job and ends with a saved `Uint8Array`.
- **Runnable starters** — full per-runtime projects in the repo's
  [`examples/runtimes/`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes)
  directory, each wired up with the correct WASM init for its environment.

## Task recipes

| Recipe | What it shows |
| --- | --- |
| [Fill & flatten a form](/examples/fill-and-flatten/) | Load an AcroForm PDF, set fields, flatten to static |
| [Generate an invoice](/examples/invoice/) | Create a document from scratch — text, lines, layout |
| [Merge PDFs](/examples/merge-pdfs/) | Combine documents; merged form fields stay fillable |

## Runnable starters

Each links to a ready-to-run project — clone the repo, `cd` in, install, run.

| Runtime | Project | Notes |
| --- | --- | --- |
| Node | [`examples/runtimes/node`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/node) | Default ESM init |
| Bun | [`examples/runtimes/bun`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/bun) | Native WASM support |
| Deno | [`examples/runtimes/deno`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/deno) | `npm:` specifier |
| Vite | [`examples/runtimes/vite`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/vite) | Browser bundler |
| webpack | [`examples/runtimes/webpack`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/webpack) | `asyncWebAssembly` rule |
| Next.js | [`examples/runtimes/nextjs`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/nextjs) | Server + client |
| Cloudflare Workers | [`examples/runtimes/cloudflare-workers`](https://github.com/ignaciano3/better-pdf/tree/master/examples/runtimes/cloudflare-workers) | Imported-WASM-module binding |

For the init pattern behind each, see the [Runtime setup](/guides/runtimes/) guide.
