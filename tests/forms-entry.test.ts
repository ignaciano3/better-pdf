import { describe, expect, test } from "bun:test";
import * as formsEntry from "../src/forms/index.ts";

// The ./forms subpath must be runtime-neutral: form/field classes, errors,
// typegen — but no PdfDocument (its load() is runtime-specific) and no WASM.
describe("forms entry", () => {
  test("exports the form API surface", () => {
    expect(formsEntry.PdfForm).toBeDefined();
    expect(formsEntry.PdfTextField).toBeDefined();
    expect(formsEntry.PdfCheckBox).toBeDefined();
    expect(formsEntry.PdfRadioGroup).toBeDefined();
    expect(formsEntry.PdfDropdown).toBeDefined();
    expect(formsEntry.PdfListBox).toBeDefined();
    expect(formsEntry.PdfSignature).toBeDefined();
    expect(formsEntry.generateFormTypes).toBeDefined();
    expect(formsEntry.PdfError).toBeDefined();
    expect(formsEntry.UnknownFieldError).toBeDefined();
    expect(formsEntry.FieldTypeError).toBeDefined();
    expect(formsEntry.InvalidOptionError).toBeDefined();
    expect(formsEntry.MaxLengthExceededError).toBeDefined();
    expect(formsEntry.MissingOnStateError).toBeDefined();
    expect(formsEntry.PdfCoreError).toBeDefined();
  });

  test("does not export PdfDocument or WASM bindings", () => {
    expect("PdfDocument" in formsEntry).toBe(false);
    expect("initializeWasm" in formsEntry).toBe(false);
    expect("readFields" in formsEntry).toBe(false);
  });
});
