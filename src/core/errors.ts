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

/**
 * Thrown when the WASM core rejects an operation at save time (e.g. XFA forms,
 * unsupported images, malformed PDFs). The original core message is preserved.
 */
export class PdfCoreError extends PdfError {}

/** Thrown when a page index is outside the document's page range. */
export class PageOutOfRangeError extends PdfError {
  constructor(readonly index: number, readonly pageCount: number) {
    super(`page ${index} out of range (document has ${pageCount} pages)`);
  }
}

/** @internal Wrap a core failure so every error this library throws is a PdfError. */
export function toPdfError(e: unknown): PdfError {
  if (e instanceof PdfError) return e;
  return new PdfCoreError(e instanceof Error ? e.message : String(e));
}
