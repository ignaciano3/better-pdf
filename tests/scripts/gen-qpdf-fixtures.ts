// Generates the fixtures under tests/fixtures/qpdf/ using the QPDF CLI as a
// well-tested *producer* (encryption matrix, object/xref streams, linearized).
// Run this on a machine with qpdf installed to (re)materialize the committed
// fixtures; the qpdf-ported test tiers that depend on them skip when absent.
//
//   bun run tests/scripts/gen-qpdf-fixtures.ts
//
// See tests/fixtures/qpdf/LICENSE.qpdf for provenance.
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
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
// The raw string has no xref table, so qpdf reconstructs it (a warning) on every
// read. Normalize it once into a clean base.pdf so the downstream --encrypt /
// object-stream calls read a well-formed file and stay quiet.
const rawBase = join(ROOT, "base-raw.pdf");
const basePath = join(ROOT, "base.pdf");
writeFileSync(rawBase, BASE);

function qpdf(args: string[], out: string) {
  const r = Bun.spawnSync(["qpdf", ...args], { stdout: "pipe", stderr: "pipe" });
  // qpdf exit 0 = clean, 3 = warnings (still wrote output); both are acceptable.
  if (r.exitCode !== 0 && r.exitCode !== 3) {
    throw new Error(`qpdf ${args.join(" ")} failed (${r.exitCode}): ${r.stderr.toString()}`);
  }
  console.log(`  wrote ${out}`);
}

// Normalize the raw base into a clean, well-formed base.pdf (qpdf reconstructs
// the missing xref here, once, instead of on every downstream call).
qpdf([rawBase, basePath], "base.pdf");

// --- Encryption matrix (mirrors QPDF's --encrypt key-length / revision cases) ---
// Signature: qpdf [globals] --encrypt <user> <owner> <bits> [opts] -- in out
// Modern qpdf (11+) refuses to *write* RC4 (the R2/R3 revisions) unless
// --allow-weak-crypto is passed — RC4 is exactly the legacy scheme a reader must
// still handle, so we opt in for those cases. AES (R4/R6) needs no such flag.
const WEAK = "--allow-weak-crypto";
console.log("encryption/");
qpdf([WEAK, "--encrypt", "", "", "40", "--", basePath, join(ENC, "r2-rc4-40-empty.pdf")], "r2-rc4-40-empty.pdf");
qpdf([WEAK, "--encrypt", "", "", "128", "--use-aes=n", "--", basePath, join(ENC, "r3-rc4-128-empty.pdf")], "r3-rc4-128-empty.pdf");
qpdf(["--encrypt", "", "", "256", "--", basePath, join(ENC, "r6-aes-256-empty.pdf")], "r6-aes-256-empty.pdf");
qpdf([WEAK, "--encrypt", "asdfzxcv", "", "40", "--", basePath, join(ENC, "r2-rc4-40-user.pdf")], "r2-rc4-40-user.pdf");
qpdf(["--encrypt", "asdfzxcv", "", "128", "--use-aes=y", "--", basePath, join(ENC, "r4-aes-128-user.pdf")], "r4-aes-128-user.pdf");
// A file with distinct non-empty user AND owner passwords, so a wrong password
// authenticates against neither (empty-owner files open as owner for any string).
qpdf(["--encrypt", "foo", "bar", "256", "--", basePath, join(ENC, "r6-both-passwords.pdf")], "r6-both-passwords.pdf");
// Encrypted files whose trailer is a cross-reference *stream* (PDF 1.5+) rather
// than a classic `trailer` dictionary — what Word/Acrobat and qpdf's
// --object-streams=generate emit. The trailer entries a reader needs to
// authenticate a password (/Encrypt, /ID) then live in the xref stream's dict.
const XREFSTM = ["--object-streams=generate", "--compress-streams=y"];
qpdf([WEAK, "--encrypt", "asdfzxcv", "", "128", "--use-aes=n", "--", ...XREFSTM, basePath, join(ENC, "r3-rc4-128-user-xrefstm.pdf")], "r3-rc4-128-user-xrefstm.pdf");
qpdf([WEAK, "--encrypt", "asdfzxcv", "", "40", "--", ...XREFSTM, basePath, join(ENC, "r2-rc4-40-user-xrefstm.pdf")], "r2-rc4-40-user-xrefstm.pdf");
qpdf(["--encrypt", "foo", "bar", "256", "--", ...XREFSTM, basePath, join(ENC, "r6-both-passwords-xrefstm.pdf")], "r6-both-passwords-xrefstm.pdf");
// A password whose SASLprep (NFKC) form differs from the bytes the user typed:
// "café" with a combining acute (NFD). qpdf keys the file off the raw UTF-8
// bytes, so a reader that only ever normalizes can never authenticate it.
// Escaped rather than literal so the combining mark survives any editor or
// tool that helpfully normalizes source files.
const NFD_PASSWORD = "cafe\u0301";
qpdf(["--encrypt", NFD_PASSWORD, "owner", "256", "--", basePath, join(ENC, "r6-nfd-password.pdf")], "r6-nfd-password.pdf");
qpdf(["--encrypt", NFD_PASSWORD, "owner", "256", "--", ...XREFSTM, basePath, join(ENC, "r6-nfd-password-xrefstm.pdf")], "r6-nfd-password-xrefstm.pdf");

// --- Structure (QPDF's object-stream / xref-stream / linearization shapes) ---
console.log("structure/");
qpdf(["--object-streams=generate", "--compress-streams=y", "--", basePath, join(STRUCT, "object-streams.pdf")], "object-streams.pdf");
qpdf(["--linearize", "--", basePath, join(STRUCT, "linearized.pdf")], "linearized.pdf");

// base.pdf is kept as the plaintext oracle for the encryption tests; drop the
// pre-normalization raw copy.
rmSync(rawBase, { force: true });
console.log("done.");
