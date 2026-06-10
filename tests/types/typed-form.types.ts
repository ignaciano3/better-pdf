// Compile-time assertions for the typed form API. This file has no runtime
// tests — it is checked by `bun run typecheck` (tsconfig includes "tests") and
// is intentionally NOT named `*.test.ts`, so `bun test` never runs it. Each
// `@ts-expect-error` line MUST fail to compile; if it ever compiles, typecheck
// fails — that is the assertion.
import { PdfDocument } from "../../src/index.ts";

const schema = {
  "applicant.name": { type: "text", readOnly: false, value: "", states: [] as const, options: [] as const },
  "applicant.status": { type: "dropdown", readOnly: false, value: "Single", states: [] as const, options: ["Single", "Married"] as const },
  "applicant.kind": { type: "radio", readOnly: false, value: "Primary", states: ["Primary", "Dependent"] as const, options: [] as const },
  "applicant.lang": { type: "listbox", readOnly: false, value: "ES", states: [] as const, options: ["ES", "EN"] as const },
  "applicant.signature": { type: "signature", readOnly: false, value: "", states: [] as const, options: [] as const },
} as const;

declare const doc: PdfDocument;
const form = doc.getForm<typeof schema>();

// Positive — these MUST compile.
form.getTextField("applicant.name").setText("Ada");
form.getDropdown("applicant.status").select("Married");
form.getRadioGroup("applicant.kind").select("Primary");
form.getListBox("applicant.lang").select("EN");
form.getSignature("applicant.signature");
form.flattenField("applicant.name");

// Negative — each next line MUST be a type error.
// @ts-expect-error unknown field name
form.getTextField("applicant.unknown");
// @ts-expect-error wrong field type (status is a dropdown, not text)
form.getTextField("applicant.status");
// @ts-expect-error invalid dropdown option
form.getDropdown("applicant.status").select("Widowed");
// @ts-expect-error invalid radio state
form.getRadioGroup("applicant.kind").select("Other");
// @ts-expect-error invalid listbox option
form.getListBox("applicant.lang").select("DE");
// @ts-expect-error wrong field type (lang is a listbox, not a dropdown)
form.getDropdown("applicant.lang");
// @ts-expect-error unknown field name for flatten
form.flattenField("nope");

// Backward compatibility — the untyped form keeps accepting plain strings.
declare const loose: PdfDocument;
loose.getForm().getTextField("anything").setText("x");
loose.getForm().getDropdown("anything").select("anyvalue");
