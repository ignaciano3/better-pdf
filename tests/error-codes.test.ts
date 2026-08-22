import { describe, expect, test } from "bun:test";
import {
  DuplicateAttachmentError,
  EncryptedPdfError,
  IncorrectPasswordError,
  MissingGlyphError,
  PdfCoreError,
  PdfError,
  toPdfError,
} from "../src/core/errors.ts";

/**
 * Conformance net for the Rust→TS error protocol. The core tags recoverable
 * conditions with a machine-readable envelope (`better-pdf-error:<code>:<detail>`);
 * this file pins every code → class mapping so a reworded core message can
 * never silently degrade typed errors into generic PdfCoreErrors.
 */
describe("core error protocol (toPdfError)", () => {
  describe("coded envelope", () => {
    const coded = (code: string, detail: string) =>
      new Error(`better-pdf-error:${code}:${detail}`);

    test("password → IncorrectPasswordError, detail preserved when present", () => {
      const err = toPdfError(coded("password", "bad password"));
      expect(err).toBeInstanceOf(IncorrectPasswordError);
      expect(err.message).toBe("bad password");
    });

    test("password with empty detail falls back to the default message", () => {
      const err = toPdfError(coded("password", ""));
      expect(err).toBeInstanceOf(IncorrectPasswordError);
      expect(err.message).toBe("incorrect or missing password for this encrypted PDF");
    });

    test("encrypted → EncryptedPdfError, detail preserved when present", () => {
      const err = toPdfError(
        coded("encrypted", "unsupported or unreadable encryption: RC5"),
      );
      expect(err).toBeInstanceOf(EncryptedPdfError);
      expect(err.message).toContain("RC5");
    });

    test("encrypted with empty detail falls back to the default message", () => {
      const err = toPdfError(coded("encrypted", ""));
      expect(err).toBeInstanceOf(EncryptedPdfError);
      expect(err.message).toBe(
        'this PDF is encrypted; load it with PdfDocument.load(bytes, { password }) (use "" for owner-locked files)',
      );
    });

    test("missing-glyphs → MissingGlyphError carrying the full detail", () => {
      const detail = 'missing glyphs in font for drawText on page 0: "㐀" (U+3400)';
      const err = toPdfError(coded("missing-glyphs", detail));
      expect(err).toBeInstanceOf(MissingGlyphError);
      expect(err.message).toBe(detail);
    });

    test("duplicate-attachment → DuplicateAttachmentError whose payload is the name", () => {
      const err = toPdfError(coded("duplicate-attachment", "same.txt"));
      expect(err).toBeInstanceOf(DuplicateAttachmentError);
      expect((err as DuplicateAttachmentError).attachmentName).toBe("same.txt");
      // Names containing quotes/colons survive intact: the detail IS the name,
      // not prose to be regexed.
      const tricky = toPdfError(coded("duplicate-attachment", "weird:name'.pdf"));
      expect((tricky as DuplicateAttachmentError).attachmentName).toBe("weird:name'.pdf");
    });

    test("unknown code surfaces verbatim as PdfCoreError (forward compatibility)", () => {
      const raw = "better-pdf-error:from-the-future:something new";
      const err = toPdfError(new Error(raw));
      expect(err).toBeInstanceOf(PdfCoreError);
      expect(err.message).toBe(raw);
    });
  });

  describe("legacy prefix fallback (pre-envelope cores)", () => {
    test("PASSWORD:", () => {
      expect(toPdfError(new Error("PASSWORD: wrong"))).toBeInstanceOf(IncorrectPasswordError);
    });
    test("ENCRYPTED:", () => {
      expect(toPdfError(new Error("ENCRYPTED: nope"))).toBeInstanceOf(EncryptedPdfError);
    });
    test("missing glyphs…", () => {
      const err = toPdfError(new Error("missing glyphs in font for x: \"㐀\" (U+3400)"));
      expect(err).toBeInstanceOf(MissingGlyphError);
    });
    test("duplicate attachment name 'x'…", () => {
      const err = toPdfError(
        new Error("duplicate attachment name 'a.pdf' already exists in the document"),
      );
      expect(err).toBeInstanceOf(DuplicateAttachmentError);
      expect((err as DuplicateAttachmentError).attachmentName).toBe("a.pdf");
    });
  });

  describe("general contract", () => {
    test("PdfError instances pass through unchanged", () => {
      const original = new EncryptedPdfError();
      expect(toPdfError(original)).toBe(original);
    });

    test("non-Error values are stringified into PdfCoreError", () => {
      const err = toPdfError(42);
      expect(err).toBeInstanceOf(PdfError);
      expect(err.message).toBe("42");
    });

    test("plain failures wrap as PdfCoreError keeping the message", () => {
      const err = toPdfError(new Error("parse failed at xref"));
      expect(err).toBeInstanceOf(PdfCoreError);
      expect(err.message).toBe("parse failed at xref");
    });
  });
});
