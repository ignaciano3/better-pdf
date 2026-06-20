#!/usr/bin/env node
/**
 * Copies better_pdf_core_bg.wasm from the installed package into public/ so
 * Next.js can serve it as a static asset at /better_pdf_core_bg.wasm.
 *
 * Run automatically via the `postinstall` script in package.json, or manually:
 *   node scripts/copy-wasm.mjs
 */
import { copyFileSync, mkdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Resolve the wasm file via import.meta.resolve (Node 18+), which respects
// the package "exports" map and handles the `./wasm` subpath correctly.
const wasmModuleUrl = import.meta.resolve("@ignaciano3/better-pdf/wasm");
const wasmSrc = fileURLToPath(wasmModuleUrl);

const publicDir = resolve(__dirname, "..", "public");
const wasmDest = resolve(publicDir, "better_pdf_core_bg.wasm");

mkdirSync(publicDir, { recursive: true });
copyFileSync(wasmSrc, wasmDest);
console.log(`Copied better_pdf_core_bg.wasm → public/`);
