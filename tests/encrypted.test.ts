import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument, IncorrectPasswordError, EncryptedPdfError } from "../src/index.ts";

const fx = (name: string) =>
  new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/generated", name)));

test("loads an RC4-encrypted PDF with an explicit empty password", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"), { password: "" });
  const names = doc.getForm().getFields().map((f) => f.name);
  expect(names).toContain("beneficiario.apellidos_nombres");
});

test("loads an AES-128-encrypted PDF with an explicit empty password", async () => {
  const doc = await PdfDocument.load(fx("ficha-aes128.pdf"), { password: "" });
  expect(doc.getForm().getFields().length).toBeGreaterThan(0);
});

test("loads a password-protected PDF with the correct password", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "secret" });
  expect(doc.getForm().getFields().length).toBeGreaterThan(0);
});

test("wrong password throws IncorrectPasswordError", async () => {
  await expect(
    PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "wrong" }),
  ).rejects.toBeInstanceOf(IncorrectPasswordError);
});

test("empty password on a password-protected PDF throws IncorrectPasswordError", async () => {
  await expect(
    PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "" }),
  ).rejects.toBeInstanceOf(IncorrectPasswordError);
});

test("an encrypted PDF loaded without a password rejects on use (opt-in)", async () => {
  // Bare load is lazy; the existing reject fires on the first operation.
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"));
  expect(() => doc.getForm().getFields()).toThrow(EncryptedPdfError);
});

test("filling an encrypted form produces a decrypted output", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"), { password: "" });
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  const out = await doc.save();
  // Reload WITHOUT a password — the output must be plain (decrypted).
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});
