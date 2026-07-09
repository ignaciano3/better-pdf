// Generates the fixtures under tests/fixtures/qpdf/ using the QPDF CLI as a
// well-tested *producer* (encryption matrix, object/xref streams, linearized).
// Run this on a machine with qpdf installed to (re)materialize the committed
// fixtures; the qpdf-ported test tiers that depend on them skip when absent.
//
//   bun run tests/scripts/gen-qpdf-fixtures.ts
//
// See tests/fixtures/qpdf/LICENSE.qpdf for provenance.
import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..", "fixtures", "qpdf");
const ENC = join(ROOT, "encryption");
const STRUCT = join(ROOT, "structure");

function haveQpdf(): boolean {
  try {
    return Bun.spawnSync(["qpdf", "--version"], { stdout: "ignore", stderr: "ignore" }).exitCode === 0;
  } catch {
    return false;
  }
}

if (!haveQpdf()) {
  console.error("qpdf not found on PATH — install it (brew/apt/dnf install qpdf) and re-run.");
  process.exit(1);
}

mkdirSync(ENC, { recursive: true });
mkdirSync(STRUCT, { recursive: true });

// A tiny, valid 1-page base document with a known /Info /Author. better-pdf and
// qpdf both reconstruct its (absent) xref; qpdf re-emits a clean file.
const BASE = `%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj
4 0 obj<</Author(qpdf-fixture)>>endobj
trailer<</Size 5/Root 1 0 R/Info 4 0 R>>
%%EOF`;
const basePath = join(ROOT, "base.pdf");
writeFileSync(basePath, BASE);

function qpdf(args: string[], out: string) {
  const r = Bun.spawnSync(["qpdf", ...args], { stdout: "pipe", stderr: "pipe" });
  // qpdf exit 0 = clean, 3 = warnings (still wrote output); both are acceptable.
  if (r.exitCode !== 0 && r.exitCode !== 3) {
    throw new Error(`qpdf ${args.join(" ")} failed (${r.exitCode}): ${r.stderr.toString()}`);
  }
  console.log(`  wrote ${out}`);
}

// --- Encryption matrix (mirrors QPDF's --encrypt key-length / revision cases) ---
// Signature: qpdf --encrypt <user> <owner> <bits> [opts] -- in out
console.log("encryption/");
qpdf(["--encrypt", "", "", "40", "--", basePath, join(ENC, "r2-rc4-40-empty.pdf")], "r2-rc4-40-empty.pdf");
qpdf(["--encrypt", "", "", "128", "--use-aes=n", "--", basePath, join(ENC, "r3-rc4-128-empty.pdf")], "r3-rc4-128-empty.pdf");
qpdf(["--encrypt", "", "", "256", "--", basePath, join(ENC, "r6-aes-256-empty.pdf")], "r6-aes-256-empty.pdf");
qpdf(["--encrypt", "asdfzxcv", "", "40", "--", basePath, join(ENC, "r2-rc4-40-user.pdf")], "r2-rc4-40-user.pdf");
qpdf(["--encrypt", "asdfzxcv", "", "128", "--use-aes=y", "--", basePath, join(ENC, "r4-aes-128-user.pdf")], "r4-aes-128-user.pdf");
// A file with distinct non-empty user AND owner passwords, so a wrong password
// authenticates against neither (empty-owner files open as owner for any string).
qpdf(["--encrypt", "foo", "bar", "256", "--", basePath, join(ENC, "r6-both-passwords.pdf")], "r6-both-passwords.pdf");

// --- Structure (QPDF's object-stream / xref-stream / linearization shapes) ---
console.log("structure/");
qpdf(["--object-streams=generate", "--compress-streams=y", "--", basePath, join(STRUCT, "object-streams.pdf")], "object-streams.pdf");
qpdf(["--linearize", "--", basePath, join(STRUCT, "linearized.pdf")], "linearized.pdf");

console.log("done.");
if (existsSync(basePath)) {
  // keep base.pdf: it's the plaintext oracle for the encryption round-trip tests.
}
