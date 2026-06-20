import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  PdfDocument,
  PdfError,
  UnknownFieldError,
  FieldTypeError,
  InvalidOptionError,
  PdfCoreError,
  EncryptedPdfError,
} from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

function loadForm() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA))).then((d) =>
    d.getForm(),
  );
}

test("unknown field throws UnknownFieldError (a PdfError)", async () => {
  const form = await loadForm();
  let err: unknown;
  try {
    form.getTextField("does.not.exist");
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(UnknownFieldError);
  expect(err).toBeInstanceOf(PdfError);
  expect((err as UnknownFieldError).field).toBe("does.not.exist");
  expect((err as Error).name).toBe("UnknownFieldError");
});

test("wrong-type access throws FieldTypeError carrying actual + expected", async () => {
  const form = await loadForm();
  let err: unknown;
  try {
    form.getDropdown("beneficiario.apellidos_nombres");
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(FieldTypeError);
  const e = err as FieldTypeError;
  expect(e.actual).toBe("text");
  expect(e.expected).toBe("dropdown");
  expect(e.message).toMatch(/not a dropdown/);
});

test("invalid option throws InvalidOptionError listing valid values", async () => {
  const form = await loadForm();
  const radio = form.getRadioGroup("beneficiario.tipo_beneficiario");
  let err: unknown;
  try {
    radio.select("definitely-not-an-option");
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(InvalidOptionError);
  const e = err as InvalidOptionError;
  expect(e.field).toBe("beneficiario.tipo_beneficiario");
  expect(e.options).toEqual(radio.options);
});

test("core failures from save() are PdfCoreError (a PdfError)", async () => {
  const bytes = new Uint8Array(
    readFileSync(join(import.meta.dir, "fixtures/generated/ficha-xfa.pdf")),
  );
  const doc = await PdfDocument.load(bytes);
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("X");
  await expect(doc.save()).rejects.toBeInstanceOf(PdfCoreError);
  await expect(doc.save()).rejects.toThrow(/XFA/);
});

test("loading an encrypted PDF throws EncryptedPdfError (a PdfError)", async () => {
  const bytes = new Uint8Array(
    readFileSync(
      join(import.meta.dir, "fixtures/generated/encrypted-min.pdf"),
    ),
  );
  const doc = await PdfDocument.load(bytes);
  // Encryption surfaces on the first read into the core.
  let err: unknown;
  try {
    doc.getForm();
  } catch (e) {
    err = e;
  }
  expect(err).toBeInstanceOf(EncryptedPdfError);
  expect(err).toBeInstanceOf(PdfError);
  expect((err as Error).name).toBe("EncryptedPdfError");
});
