# Milestone 12 — Typed Form API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let callers pass a generated form schema to `doc.getForm<typeof myFormFields>()` so field-name and field-value operations are checked and narrowed at compile time.

**Architecture:** A pure, type-only overlay. The runtime `PdfForm` is unchanged; a generic `getForm<S>()` overload returns a `TypedPdfForm<S>` *view* of the very same object. Field accessors are narrowed by mapped/conditional helper types derived from `typeof myFormFields` (the `as const` metadata the M11 generator already emits). Zero runtime cost, fully tree-shakeable — the schema is only ever referenced via `typeof`.

**Tech Stack:** TypeScript (strict), Bun test, `tsc --noEmit` for compile-time assertions.

---

## Why this milestone

M11 emits typed metadata but stops there: `doc.getForm()` is still untyped, so nothing prevents filling a non-existent field, reading `.options` off a text field, or selecting an invalid dropdown value. `PLAN.md` (lines 67–73) asks for exactly this narrowing — "we will 100% be sure we are not filling inexistent fields", "Options from a dropdown should be typed", "optimistically fill the field without checking which type it is". This milestone wires the generated types into the runtime API to deliver that.

**Decision (confirmed with user):** type-only generic — `doc.getForm<typeof myFormFields>()` — not a runtime-passed const. No runtime validation that the PDF matches the schema; the types are trusted (optimistic) and cost nothing at runtime.

## Scope for this slice

- Generic helper types that turn `typeof myFormFields` into narrowed accessor signatures.
- A `getForm<S>()` overload on both the Node and browser `PdfDocument`, preserving the existing untyped `getForm()` behaviour exactly.
- Make `PdfDropdown` / `PdfRadioGroup` generic over their option/state value type so `.select()` is narrowed.
- Compile-time assertion test (positive + `@ts-expect-error` negatives) and a runtime parity test.
- Docs + a one-line usage hint in generated files.

## Deferred

- A `listbox` accessor (`PdfForm` has none today; out of scope until the runtime gains one).
- Passing the metadata const at runtime / runtime schema validation.
- Convenience bulk helpers (e.g. `form.fill({ ... })`).
- Making the M11 CLI emit a ready-to-import wrapper module.

## File structure

- **Create `src/schema.ts`** — the generic type vocabulary: `FieldMeta`, `FormSchema`, `FieldNameOf`, `NameOfType`, `OptionsOf`, and the `TypedPdfForm<S>` interface. Types only, no runtime code.
- **Modify `src/fields.ts`** — make `PdfDropdown` and `PdfRadioGroup` generic (`<Opt extends string = string>`); runtime behaviour unchanged.
- **Modify `src/index.ts` and `src/index.browser.ts`** — add the `getForm<S>()` overload and re-export the schema types. The two files duplicate `PdfDocument` (pre-existing); apply the same change to both.
- **Create `tests/types/typed-form.types.ts`** — compile-time assertions, enforced by `bun run typecheck`. Named so Bun's `*.test.ts` glob does **not** run it.
- **Create `tests/typed-form.test.ts`** — runtime parity test (typed accessors reach the same runtime as untyped).
- **Modify `README.md`, `src/typegen.ts`** — document typed usage; add a usage-hint comment to generated output.

---

## Task 1: Schema type vocabulary

**Files:**
- Create: `src/schema.ts`

- [ ] **Step 1: Create the helper types**

```ts
import type { FieldInfo, FieldType } from "./form.js";
import type {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfSignature,
} from "./fields.js";

/** The compile-time shape of one generated field's metadata entry. */
export interface FieldMeta {
  type: FieldType;
  readOnly: boolean;
  value: string | null;
  states: readonly string[];
  options: readonly string[];
}

/** The shape of a generated `…Fields` metadata object (i.e. `typeof myFormFields`). */
export type FormSchema = Record<string, FieldMeta>;

/** Every field name declared in a schema. */
export type FieldNameOf<S extends FormSchema> = Extract<keyof S, string>;

/** The names in a schema whose field type is exactly `K`. */
export type NameOfType<S extends FormSchema, K extends FieldType> = {
  [N in keyof S]: S[N]["type"] extends K ? N : never;
}[keyof S] &
  string;

/** Valid values for a choice field: its options (dropdown) or its on-states (radio). */
export type OptionsOf<S extends FormSchema, N extends keyof S> =
  | S[N]["options"][number]
  | S[N]["states"][number];

/**
 * A compile-time-narrowed view over a `PdfForm`, produced by
 * `doc.getForm<typeof myFormFields>()`. This is purely a type overlay: the
 * runtime object is the same untyped `PdfForm`.
 */
export interface TypedPdfForm<S extends FormSchema> {
  getFields(): FieldInfo[];
  getField(name: FieldNameOf<S>): FieldInfo | undefined;
  getTextField(name: NameOfType<S, "text">): PdfTextField;
  getCheckBox(name: NameOfType<S, "checkbox">): PdfCheckBox;
  getRadioGroup<N extends NameOfType<S, "radio">>(name: N): PdfRadioGroup<OptionsOf<S, N>>;
  getDropdown<N extends NameOfType<S, "dropdown">>(name: N): PdfDropdown<OptionsOf<S, N>>;
  getSignature(name: NameOfType<S, "signature">): PdfSignature;
  flattenField(name: FieldNameOf<S>): void;
  flatten(): void;
}
```

- [ ] **Step 2: Verify it type-checks**

Run: `bun run typecheck`
Expected: PASS (no errors). `src/schema.ts` references `PdfDropdown<…>`/`PdfRadioGroup<…>` with a type argument, which still compiles because the next task makes them generic — but with the current non-generic classes TypeScript reports `Type 'PdfDropdown' is not generic`. **Expected here: FAIL with "is not generic"** until Task 2. Proceed to Task 2.

- [ ] **Step 3: Commit**

```bash
git add src/schema.ts
git commit -m "feat(schema): generic type vocabulary for the typed form API"
```

## Task 2: Generic option types on choice fields

**Files:**
- Modify: `src/fields.ts`

- [ ] **Step 1: Make `PdfRadioGroup` generic**

Replace the class header and `select` signature (runtime body unchanged):

```ts
/** A radio-button group. `Opt` is its set of valid export values. */
export class PdfRadioGroup<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid export values for this group. */
  get options(): string[] {
    return this.info.states;
  }
  /** Select an option by its real export value. */
  select(value: Opt): void {
    if (!this.info.states.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for radio '${this.info.name}' (valid: ${this.info.states.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}
```

- [ ] **Step 2: Make `PdfDropdown` generic**

```ts
/** A dropdown (choice) field. `Opt` is its set of valid option values. */
export class PdfDropdown<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid option export values. */
  get options(): string[] {
    return this.info.options;
  }
  /** Select an option by its real export value. */
  select(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for dropdown '${this.info.name}' (valid: ${this.info.options.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}
```

- [ ] **Step 3: Verify existing runtime tests and types still pass**

Run: `bun test && bun run typecheck`
Expected: PASS. The `= string` defaults mean `new PdfDropdown(info, queue)` is still `PdfDropdown<string>`, so `src/form.ts`, `src/fields.ts`, and existing fill tests are unaffected. `src/schema.ts` now compiles (the classes are generic).

- [ ] **Step 4: Commit**

```bash
git add src/fields.ts
git commit -m "feat(fields): make PdfDropdown/PdfRadioGroup generic over their value type"
```

## Task 3: `getForm<S>()` overload + exports

**Files:**
- Modify: `src/index.ts`
- Modify: `src/index.browser.ts`

- [ ] **Step 1: Add the overload in `src/index.ts`**

Add the import near the top:

```ts
import type { FormSchema, TypedPdfForm } from "./schema.js";
```

Replace the existing `getForm()` method with the overloaded form:

```ts
  /**
   * The document's AcroForm. The same instance is returned each call, so queued
   * mutations accumulate until `save()`.
   */
  getForm(): PdfForm;
  /**
   * A compile-time-narrowed view of the form. Pass a generated schema as the
   * type argument: `doc.getForm<typeof myFormFields>()`. Type-only — the runtime
   * object is identical to the untyped `getForm()`.
   */
  getForm<S extends FormSchema>(): TypedPdfForm<S>;
  getForm(): PdfForm {
    if (!this.form) this.form = new PdfForm(this.bytes, readFields);
    return this.form;
  }
```

Add the re-export alongside the other exports at the bottom:

```ts
export type {
  FormSchema,
  FieldNameOf,
  NameOfType,
  OptionsOf,
  TypedPdfForm,
} from "./schema.js";
```

- [ ] **Step 2: Mirror the change in `src/index.browser.ts`**

Apply the identical overload (the browser `getForm()` body uses the same `new PdfForm(this.bytes, readFields)`) and the identical `export type { … } from "./schema.js";` block, plus `import type { FormSchema, TypedPdfForm } from "./schema.js";`.

- [ ] **Step 3: Verify**

Run: `bun run typecheck && bun test`
Expected: PASS. The non-generic `getForm(): PdfForm` overload still matches the untyped call sites in `tests/` and `examples/`; the generic overload is selected only when a type argument is supplied.

- [ ] **Step 4: Commit**

```bash
git add src/index.ts src/index.browser.ts
git commit -m "feat: add typed getForm<S>() overload on both entry points"
```

## Task 4: Compile-time assertion test

**Files:**
- Create: `tests/types/typed-form.types.ts`

- [ ] **Step 1: Write the assertions**

```ts
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
  "applicant.signature": { type: "signature", readOnly: false, value: "", states: [] as const, options: [] as const },
} as const;

declare const doc: PdfDocument;
const form = doc.getForm<typeof schema>();

// Positive — these MUST compile.
form.getTextField("applicant.name").setText("Ada");
form.getDropdown("applicant.status").select("Married");
form.getRadioGroup("applicant.kind").select("Primary");
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
// @ts-expect-error unknown field name for flatten
form.flattenField("nope");

// Backward compatibility — the untyped form keeps accepting plain strings.
declare const loose: PdfDocument;
loose.getForm().getTextField("anything").setText("x");
loose.getForm().getDropdown("anything").select("anyvalue");
```

- [ ] **Step 2: Verify the assertions hold**

Run: `bun run typecheck`
Expected: PASS. (If any `@ts-expect-error` line were actually valid, TypeScript would report TS2578 "Unused '@ts-expect-error' directive" and typecheck would fail.)

- [ ] **Step 3: Sanity-check that a positive line really is checked**

Temporarily change `form.getDropdown("applicant.status").select("Married");` to `.select("Nope");` and run `bun run typecheck`.
Expected: FAIL with TS2345 on that line. Then revert the change.

- [ ] **Step 4: Confirm Bun does not execute the file as a test**

Run: `bun test 2>&1 | tail -3`
Expected: the suite runs and passes; `tests/types/typed-form.types.ts` is not listed among executed files.

- [ ] **Step 5: Commit**

```bash
git add tests/types/typed-form.types.ts
git commit -m "test(typed-form): compile-time narrowing and backward-compat assertions"
```

## Task 5: Runtime parity test

**Files:**
- Create: `tests/typed-form.test.ts`

- [ ] **Step 1: Write the test**

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

// A hand-written schema standing in for a generated `…Fields` const, with names
// that exist in the FICHA fixture so the typed accessors reach the real runtime.
const schema = {
  "beneficiario.apellidos_nombres": { type: "text", readOnly: false, value: "", states: [] as const, options: [] as const },
  "beneficiario.estado_civil": { type: "dropdown", readOnly: false, value: "Soltero", states: [] as const, options: ["Soltero", "Casado"] as const },
} as const;

function load() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
}

test("typed getForm<S>() drives the same runtime as the untyped form", async () => {
  const doc = await load();
  const form = doc.getForm<typeof schema>();
  form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  form.getDropdown("beneficiario.estado_civil").select("Casado");

  const reloaded = await PdfDocument.load(await doc.save());
  const read = reloaded.getForm();
  expect(read.getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
  expect(read.getField("beneficiario.estado_civil")?.value).toBe("Casado");
});
```

- [ ] **Step 2: Run it**

Run: `bun test tests/typed-form.test.ts`
Expected: PASS (1 test). Proves the typed overload returns the same accumulating runtime `PdfForm`.

- [ ] **Step 3: Commit**

```bash
git add tests/typed-form.test.ts
git commit -m "test(typed-form): runtime parity between typed and untyped getForm"
```

## Task 6: Docs and generated-file usage hint

**Files:**
- Modify: `src/typegen.ts`
- Modify: `README.md`

- [ ] **Step 1: Add a usage hint to generated output**

In `generateFormTypes`, extend the header `lines` array (the `metadataName` variable is already in scope above it):

```ts
  const lines: string[] = [
    "/* eslint-disable */",
    "/* Generated by better-pdf. Do not edit by hand. */",
    `/* Usage: const form = doc.getForm<typeof ${metadataName}>(); */`,
    "",
    `export const ${metadataName} = {`,
  ];
```

- [ ] **Step 2: Extend the generator test**

In `tests/typegen.test.ts`, add to the first test:

```ts
  expect(source).toContain("/* Usage: const form = doc.getForm<typeof anexoFormFields>(); */");
```

Run: `bun test tests/typegen.test.ts`
Expected: PASS.

- [ ] **Step 3: Document typed usage in the README**

Under the existing "Generate Form Types" section, append:

```md
Then use the generated metadata as a type argument to get a fully-narrowed form —
unknown field names, wrong-type access, and invalid option/state values become
compile errors, with zero runtime cost (the schema is referenced only via `typeof`):

​```ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado"); // only valid options compile
​```

The untyped `doc.getForm()` keeps working unchanged.
```

(Replace the zero-width characters around the fenced block with real backticks when writing the file.)

- [ ] **Step 4: Verify everything**

Run: `bun test && bun run typecheck && bun run build:js`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/typegen.ts tests/typegen.test.ts README.md
git commit -m "docs(typegen): emit getForm<typeof …>() usage hint and document typed forms"
```

---

## Verification (end-to-end)

1. **Type assertions:** `bun run typecheck` passes — proves positive narrowing compiles and every `@ts-expect-error` negative genuinely errors.
2. **Runtime parity:** `bun test` passes (all prior tests + `tests/typed-form.test.ts`) — proves the type overlay is additive and the typed path reaches the same runtime.
3. **Build:** `bun run build:js` emits clean declarations; spot-check that `dist/schema.d.ts` exists and `dist/index.d.ts` re-exports the schema types.
4. **Generator hint:** `node dist/bin/generate-types.js tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf` output includes the `Usage: … getForm<typeof …>()` comment.
5. **Manual smoke (optional):** generate a real types module, write a scratch `.ts` that imports it and calls `doc.getForm<typeof …>()`, and confirm an invalid option is flagged by `tsc`.

## Self-review notes

- **Spec coverage:** name-existence (Task 1/3 `FieldNameOf`/`NameOfType`), typed dropdown options + "no options off a text field" (Task 2 + `OptionsOf`, Task 4 negatives), optimistic typed fill (type-only overload, no runtime check) — all covered.
- **Type consistency:** `NameOfType`, `OptionsOf`, `FieldNameOf`, `TypedPdfForm`, `FormSchema`, `FieldMeta` are spelled identically across `src/schema.ts`, the exports in Task 3, and both test files. `PdfDropdown<Opt>` / `PdfRadioGroup<Opt>` use the same `Opt extends string = string` parameter in Task 2 and Task 1's interface.
- **Backward compatibility:** the non-generic `getForm(): PdfForm` overload is declared first and the `= string` generic defaults keep every existing untyped call site (`tests/fill.test.ts`, `tests/flatten.test.ts`, `examples/playground.ts`) compiling and behaving as before — asserted explicitly in Task 4 Step 1.
