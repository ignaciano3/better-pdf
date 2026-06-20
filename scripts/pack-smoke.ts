/**
 * pack-smoke.ts — distribution smoke test
 *
 * Builds the package, packs it, installs the tarball into a temp dir,
 * then runs a create+draw+save assertion under BOTH Node and Bun.
 * Also verifies that the ./wasm export subpath resolves correctly.
 * Exits non-zero on any failure. Cleans up the temp dir in all cases.
 *
 * All subprocess calls use execFileSync/spawnSync with argument arrays
 * (never shell: true) to avoid command-injection.
 */

import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync, existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

const ROOT = resolve(import.meta.dir, "..");

/** Run a program with an explicit arg array; throw on non-zero exit. */
function runArgs(prog: string, args: string[], cwd = ROOT): string {
  console.log(`  $ ${prog} ${args.join(" ")}`);
  return execFileSync(prog, args, { cwd, encoding: "utf-8" });
}

/** Capture stdout/stderr from a program; never uses a shell. */
function capture(prog: string, args: string[], cwd: string) {
  const result = spawnSync(prog, args, { cwd, encoding: "utf-8" });
  return {
    ok: result.status === 0,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

// ── Step 1: build ──────────────────────────────────────────────────────────
console.log("\n[pack-smoke] Building package...");
runArgs("bun", ["run", "build"]);

// ── Step 2: pack ───────────────────────────────────────────────────────────
console.log("[pack-smoke] Packing tarball...");
const packOutput = runArgs("npm", ["pack", "--pack-destination", ROOT]);
// npm pack prints the filename on the last non-empty line
const tgzName = packOutput.trim().split("\n").filter(Boolean).at(-1)!.trim();
const tgzPath = join(ROOT, tgzName);
console.log(`  tarball: ${tgzPath}`);

// ── Step 3: install into temp dir ──────────────────────────────────────────
const tmpDir = mkdtempSync(join(tmpdir(), "pack-smoke-"));
console.log(`[pack-smoke] Temp dir: ${tmpDir}`);

try {
  // Minimal package.json so npm install is happy in the temp dir
  writeFileSync(
    join(tmpDir, "package.json"),
    JSON.stringify({ name: "smoke-test", version: "1.0.0", type: "module" }),
  );

  console.log("[pack-smoke] Installing tarball...");
  runArgs("npm", ["install", "--prefer-offline", tgzPath], tmpDir);

  // ── Step 4: verify ./wasm subpath ────────────────────────────────────────
  console.log("[pack-smoke] Checking ./wasm subpath...");
  const wasmPath = join(
    tmpDir,
    "node_modules/@ignaciano3/better-pdf/pkg-web/better_pdf_core_bg.wasm",
  );
  if (!existsSync(wasmPath)) {
    throw new Error(`./wasm file not found in tarball install at: ${wasmPath}`);
  }

  const pkgJsonPath = join(tmpDir, "node_modules/@ignaciano3/better-pdf/package.json");
  const pkgJson = JSON.parse(readFileSync(pkgJsonPath, "utf-8"));
  if (pkgJson.exports?.["./wasm"] !== "./pkg-web/better_pdf_core_bg.wasm") {
    throw new Error(
      `./wasm export subpath missing or wrong in installed package.json: ` +
        JSON.stringify(pkgJson.exports?.["./wasm"]),
    );
  }
  console.log("  VERIFIED: ./wasm export subpath resolves correctly");

  // ── Step 5: write the smoke ESM script ───────────────────────────────────
  const smokeScript = `
import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);
page.drawText("Pack smoke test", {
  x: 72,
  y: 700,
  size: 18,
  font: StandardFonts.HelveticaBold,
  color: rgb(0, 0, 0),
});
const out = await doc.save();
const header = new TextDecoder("latin1").decode(out.slice(0, 5));
if (header !== "%PDF-") {
  throw new Error("Output does not start with %PDF-, got: " + JSON.stringify(header));
}
console.log("PDF output size:", out.length, "bytes — header:", header);
console.log("SMOKE OK");
`;
  const smokeFile = join(tmpDir, "smoke.mjs");
  writeFileSync(smokeFile, smokeScript);

  // ── Step 6: run under Node ────────────────────────────────────────────────
  console.log("\n[pack-smoke] Running under Node...");
  const nodeResult = capture("node", [smokeFile], tmpDir);
  if (nodeResult.stdout) process.stdout.write(nodeResult.stdout);
  if (nodeResult.stderr) process.stderr.write(nodeResult.stderr);
  if (!nodeResult.ok || !nodeResult.stdout.includes("SMOKE OK")) {
    throw new Error("Node smoke test FAILED");
  }
  console.log("VERIFIED: node");

  // ── Step 7: run under Bun ────────────────────────────────────────────────
  console.log("\n[pack-smoke] Running under Bun...");
  const bunResult = capture("bun", ["run", smokeFile], tmpDir);
  if (bunResult.stdout) process.stdout.write(bunResult.stdout);
  if (bunResult.stderr) process.stderr.write(bunResult.stderr);
  if (!bunResult.ok || !bunResult.stdout.includes("SMOKE OK")) {
    throw new Error("Bun smoke test FAILED");
  }
  console.log("VERIFIED: bun");

  console.log("\n[pack-smoke] All checks passed.");
} finally {
  // ── Cleanup ───────────────────────────────────────────────────────────────
  console.log("[pack-smoke] Cleaning up...");
  rmSync(tmpDir, { recursive: true, force: true });
  rmSync(tgzPath, { force: true });
  console.log("[pack-smoke] Done.");
}
