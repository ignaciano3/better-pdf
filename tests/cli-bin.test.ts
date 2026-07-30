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
    "Usage: better-pdf-generate-types <input.pdf> [output.ts] [--name TypeName] [--password PW]",
  );
});

const cliPath = join(import.meta.dir, "../src/bin/generate-types.ts");
const fixture = (name: string) => join(import.meta.dir, "fixtures/generated", name);

test("generates types for an encrypted PDF when given --password", () => {
  const proc = Bun.spawnSync([
    process.execPath,
    cliPath,
    fixture("ficha-rc4-pw.pdf"),
    "--name",
    "Secured",
    "--password",
    "secret",
  ]);

  expect(proc.exitCode).toBe(0);
  expect(new TextDecoder().decode(proc.stdout)).toContain("export const securedFields = {");
});

test("an owner-locked PDF opens with an empty --password", () => {
  const proc = Bun.spawnSync([process.execPath, cliPath, fixture("ficha-rc4.pdf"), "--password", ""]);

  expect(proc.exitCode).toBe(0);
  expect(new TextDecoder().decode(proc.stdout)).toContain("export const betterPdfFormFields = {");
});

test("an encrypted PDF without --password fails with the encryption error", () => {
  const proc = Bun.spawnSync([process.execPath, cliPath, fixture("ficha-rc4-pw.pdf")]);

  expect(proc.exitCode).toBe(1);
  expect(new TextDecoder().decode(proc.stderr)).toContain("encrypted");
});

test("a wrong --password fails rather than emitting an empty schema", () => {
  const proc = Bun.spawnSync([
    process.execPath,
    cliPath,
    fixture("ficha-rc4-pw.pdf"),
    "--password",
    "nope",
  ]);

  expect(proc.exitCode).toBe(1);
  expect(new TextDecoder().decode(proc.stderr)).toContain("password");
});
