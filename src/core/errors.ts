import type { FieldType } from "../forms/form.js";

/**
 * Base class for every error `better-pdf` throws from the form API. Catch this
 * to handle any library error, or one of the subclasses for a specific case.
 */
export class PdfError extends Error {
  constructor(message: string) {
    super(message);
    // Use the concrete subclass name (UnknownFieldError, ...) even after the
    // class hierarchy is minified down to a single constructor at runtime.
    this.name = new.target.name;
  }
}

/** Thrown when a field name does not exist in the form. */
export class UnknownFieldError extends PdfError {
  constructor(readonly field: string) {
    super(`no such field: ${field}`);
  }
}

/** Thrown when a field is accessed as the wrong type (e.g. dropdown vs text). */
export class FieldTypeError extends PdfError {
  constructor(
    readonly field: string,
    readonly actual: FieldType,
    readonly expected: FieldType,
  ) {
    super(`field '${field}' is a ${actual}, not a ${expected}`);
  }
}

/** Thrown when selecting a value that is not one of a field's valid options. */
export class InvalidOptionError extends PdfError {
  constructor(
    readonly field: string,
    readonly fieldType: FieldType,
    readonly value: string,
    readonly options: readonly string[],
  ) {
    super(
      `'${value}' is not a valid option for ${fieldType} '${field}' (valid: ${options.join(", ")})`,
    );
  }
}

/** Thrown when setting text longer than a field's declared `/MaxLen`. */
export class MaxLengthExceededError extends PdfError {
  constructor(
    readonly field: string,
    readonly maxLength: number,
    readonly actualLength: number,
  ) {
    super(
      `text for '${field}' is ${actualLength} chars, exceeding its max length of ${maxLength}`,
    );
  }
}

/** Thrown when checking a checkbox that declares no on-state in its widgets. */
export class MissingOnStateError extends PdfError {
  constructor(readonly field: string) {
    super(`checkbox '${field}' has no on-state`);
  }
}

/** Thrown when `selectMultiple` is called on a single-select list box. */
export class MultiSelectError extends PdfError {
  constructor(readonly field: string) {
    super(`list box '${field}' is single-select; use select() instead of selectMultiple()`);
  }
}

/**
 * Thrown when the WASM core rejects an operation at save time (e.g. XFA forms,
 * unsupported images, malformed PDFs). The original core message is preserved.
 */
export class PdfCoreError extends PdfError {}

/** Thrown when text contains characters the embedded font has no glyph for. */
export class MissingGlyphError extends PdfError {
  constructor(readonly detail: string) {
    super(detail); // detail is the core message: 'missing glyphs in font for …: "㐀" (U+3400)'
  }
}

/**
 * Thrown when loading or operating on an encrypted PDF. Encryption is not
 * supported; the document must be decrypted before use.
 */
export class EncryptedPdfError extends PdfError {
  constructor(
    message = "this PDF is encrypted; load it with PdfDocument.load(bytes, { password }) (use \"\" for owner-locked files)",
  ) {
    super(message);
  }
}

/** Thrown when an encrypted PDF's password is wrong or missing. Pass the
 * correct password via `PdfDocument.load(bytes, { password })`. */
export class IncorrectPasswordError extends PdfError {
  constructor(
    message = "incorrect or missing password for this encrypted PDF",
  ) {
    super(message);
  }
}

/** Thrown when a page index is outside the document's page range. */
export class PageOutOfRangeError extends PdfError {
  constructor(readonly index: number, readonly pageCount: number) {
    super(`page ${index} out of range (document has ${pageCount} pages)`);
  }
}

/** Thrown when image bytes are not a supported JPEG or PNG. */
export class InvalidImageError extends PdfError {}

/** Thrown when a rotation value is not a multiple of 90 degrees. */
export class InvalidRotationError extends PdfError {
  constructor(readonly degrees: number) {
    super(`rotation must be a multiple of 90 degrees, got ${degrees}`);
  }
}

/**
 * Thrown when field, page, or draw operations are attempted on a created
 * document after `getForm()` has sealed it. Do all content creation before
 * calling `getForm()`.
 */
export class FormSealedError extends PdfError {
  constructor(
    message = "content creation is sealed after getForm() on a created document; add all fields, pages, and drawings before calling getForm().",
  ) {
    super(message);
  }
}

export function toInvalidImageError(e: unknown): InvalidImageError {
  const message = e instanceof Error ? e.message : String(e);
  return new InvalidImageError(message);
}

/** @internal Wrap a core failure so every error this library throws is a PdfError. */
export function toPdfError(e: unknown): PdfError {
  if (e instanceof PdfError) return e;
  const message = e instanceof Error ? e.message : String(e);
  if (message.includes("PASSWORD:")) return new IncorrectPasswordError();
  if (message.includes("ENCRYPTED:")) return new EncryptedPdfError();
  if (message.startsWith("missing glyphs")) return new MissingGlyphError(message);
  return new PdfCoreError(message);
}
