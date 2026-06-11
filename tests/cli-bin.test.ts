import { expect, test } from "bun:test";
import { mkdtempSync, symlinkSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

test("generate-types CLI runs when invoked through a package bin symlink", () => {
  const dir = mkdtempSync(join(tmpdir(), "better-pdf-cli-"));
  const binPath = join(dir, "better-pdf-generate-types");
  symlinkSync(join(import.meta.dir, "../src/bin/generate-types.ts"), binPath);

  const proc = Bun.spawnSync([process.execPath, binPath, "--help"]);

  expect(proc.exitCode).toBe(0);
  expect(new TextDecoder().decode(proc.stdout)).toContain(
    "Usage: better-pdf-generate-types <input.pdf> [output.ts] [--name TypeName]",
  );
});
