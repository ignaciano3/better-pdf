# Milestone 19 — Restructure into core/ and forms/ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize `src/` into `core/` and `forms/` directories, add a runtime-neutral `./forms` subpath export, and delete the 17 unreferenced fixture PDFs — with zero behavior change.

**Architecture:** Pure file moves plus import-specifier updates. The two entry files (`src/index.ts`, `src/index.browser.ts`) stay at the root and keep exporting the exact same surface; a new `src/forms/index.ts` barrel backs the `./forms` subpath. No Rust changes; the kept fixtures are exactly the ones `crates/core` tests, the fuzz workflow, and the TS tests reference.

**Tech Stack:** TypeScript (ESM, `.js` specifiers), bun test, tsc, git mv.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md`

---

### Task 1: Delete unused fixtures

**Files:**
- Delete: 17 PDFs under `tests/fixtures/` (exact list below)

- [ ] **Step 1: Verify the unused set is still unused**

Run (from repo root):

```bash
for f in \
  "tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 2.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.-P.-2.4.5-Consentimiento-informado-prestador-padres-tutor.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.-P.-2.4.6-Consentimiento-informado-para-transporte-prestador-padres-tutor.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.2-Resumen-de-historia-clinica.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.3-Consentimiento-informado-conformidad.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.4-Solicitud-de-transporte.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.7-Medida-de-Independencia-Funcional.pdf" \
  "tests/fixtures/Medicamentos/Form-DP-2-11-2-Solicitud-de-Medicacamentos-Anticonceptivos-orales.pdf" \
  "tests/fixtures/Medicamentos/Formulario-de-medicacion-de-pacientes-cronicos.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloAdicciones.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloFertilizacionAsistida.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloHIV-SIDA.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloHepatitis.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloIntervenciondeadecuaciondegenitalidad.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloQuirurgicoeInternaciones.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloTrasplantedeorganos.pdf" \
  "tests/fixtures/Patologias Especiales/ModulodeMedicaciondeAltoCosto.pdf"; do
  base=$(basename "$f")
  hits=$(grep -rlF "$base" tests scripts src bench examples skills docs crates/core/src .github README.md PLAN.md CHANGELOG.md 2>/dev/null | grep -v 'docs/superpowers' || true)
  [ -n "$hits" ] && echo "STILL REFERENCED: $f -> $hits"
done; echo done
```

Expected: only `done` printed. If any `STILL REFERENCED` line appears, stop and remove that file from the deletion list.

- [ ] **Step 2: Delete the files**

```bash
git rm \
  "tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 2.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.-P.-2.4.5-Consentimiento-informado-prestador-padres-tutor.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.-P.-2.4.6-Consentimiento-informado-para-transporte-prestador-padres-tutor.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.2-Resumen-de-historia-clinica.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.3-Consentimiento-informado-conformidad.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.4-Solicitud-de-transporte.pdf" \
  "tests/fixtures/Discapacidad/Form.-D.P.-2.4.7-Medida-de-Independencia-Funcional.pdf" \
  "tests/fixtures/Medicamentos/Form-DP-2-11-2-Solicitud-de-Medicacamentos-Anticonceptivos-orales.pdf" \
  "tests/fixtures/Medicamentos/Formulario-de-medicacion-de-pacientes-cronicos.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloAdicciones.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloFertilizacionAsistida.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloHIV-SIDA.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloHepatitis.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloIntervenciondeadecuaciondegenitalidad.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloQuirurgicoeInternaciones.pdf" \
  "tests/fixtures/Patologias Especiales/ModuloTrasplantedeorganos.pdf" \
  "tests/fixtures/Patologias Especiales/ModulodeMedicaciondeAltoCosto.pdf"
rmdir "tests/fixtures/Patologias Especiales"
```

Expected: `tests/fixtures` now contains 7 files: `Asistencia al Viajero/Formulario asistencia al viajero 1.pdf`, `Discapacidad/Anexo-3-sssalud.pdf`, `Discapacidad/Convenio-OSFATUN-Discapacidad-2022.pdf`, `Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf`, `Medicamentos/Modulo-de-Diabetes.pdf`, `generated/ficha-objstreams.pdf`, `generated/ficha-xfa.pdf`.

- [ ] **Step 3: Run TS tests and Rust tests**

```bash
bun test
cargo test --manifest-path crates/core/Cargo.toml
```

Expected: all pass (Rust tests use `include_bytes!` on kept fixtures only).

- [ ] **Step 4: Commit**

```bash
git commit -m "chore: remove 17 unused fixture PDFs"
```

---

### Task 2: Move sources into core/ and forms/

**Files:**
- Move: `src/errors.ts` → `src/core/errors.ts`
- Move: `src/wasm.ts` → `src/core/wasm.ts`
- Move: `src/wasm-browser.ts` → `src/core/wasm-browser.ts`
- Move: `src/form.ts` → `src/forms/form.ts`
- Move: `src/fields.ts` → `src/forms/fields.ts`
- Move: `src/schema.ts` → `src/forms/schema.ts`
- Move: `src/typegen.ts` → `src/forms/typegen.ts`
- Modify: `src/index.ts`, `src/index.browser.ts`, `src/bin/generate-types.ts`
- Modify: `tests/fillqueue.test.ts`, `tests/listbox.test.ts`, `tests/typegen.test.ts`, `tests/text-maxlen.test.ts`, `bench/bench.ts`

All moves and import fixes land in ONE commit — the intermediate states do not compile, so do not run tests until Step 5.

- [ ] **Step 1: git mv the seven files**

```bash
mkdir -p src/core src/forms
git mv src/errors.ts src/core/errors.ts
git mv src/wasm.ts src/core/wasm.ts
git mv src/wasm-browser.ts src/core/wasm-browser.ts
git mv src/form.ts src/forms/form.ts
git mv src/fields.ts src/forms/fields.ts
git mv src/schema.ts src/forms/schema.ts
git mv src/typegen.ts src/forms/typegen.ts
```

- [ ] **Step 2: Fix imports inside the moved files**

Exact specifier changes (everything else in these files stays untouched):

`src/core/errors.ts` line 1:
```ts
// before
import type { FieldType } from "./form.js";
// after
import type { FieldType } from "../forms/form.js";
```

`src/core/wasm.ts` (two places — the import and the `new URL` path, both gain one `../` because the file is one level deeper):
```ts
// before
} from "../pkg-web/better_pdf_core.js";
...
  module: readFileSync(new URL("../pkg-web/better_pdf_core_bg.wasm", import.meta.url)),
// after
} from "../../pkg-web/better_pdf_core.js";
...
  module: readFileSync(new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url)),
```

`src/core/wasm-browser.ts` (two places — the import on line 8 and the default `new URL` path on line 16):
```ts
// before
} from "../pkg-web/better_pdf_core.js";
...
      moduleOrPath ?? new URL("../pkg-web/better_pdf_core_bg.wasm", import.meta.url);
// after
} from "../../pkg-web/better_pdf_core.js";
...
      moduleOrPath ?? new URL("../../pkg-web/better_pdf_core_bg.wasm", import.meta.url);
```

`src/forms/form.ts` line 10 (the `./fields.js` import on lines 1–9 stays):
```ts
// before
import { UnknownFieldError, FieldTypeError } from "./errors.js";
// after
import { UnknownFieldError, FieldTypeError } from "../core/errors.js";
```

`src/forms/fields.ts` lines 2–6 (the `./form.js` type import stays):
```ts
// before
} from "./errors.js";
// after
} from "../core/errors.js";
```

`src/forms/schema.ts` and `src/forms/typegen.ts`: no changes — they only import `./form.js` / `./fields.js`, which moved with them.

- [ ] **Step 3: Fix the three entry/bin files**

`src/index.ts` — update every relative specifier (8 import/export sites):

| before | after |
|---|---|
| `"./wasm.js"` | `"./core/wasm.js"` |
| `"./form.js"` (1 import + 2 exports) | `"./forms/form.js"` |
| `"./errors.js"` (1 import + 1 export) | `"./core/errors.js"` |
| `"./schema.js"` (1 import + 1 export) | `"./forms/schema.js"` |
| `"./fields.js"` | `"./forms/fields.js"` |
| `"./typegen.js"` (2 exports) | `"./forms/typegen.js"` |

`src/index.browser.ts` — same table, plus:

| before | after |
|---|---|
| `"./wasm-browser.js"` (1 import + 1 export) | `"./core/wasm-browser.js"` |

`src/bin/generate-types.ts` line 5:
```ts
// before
import { generateFormTypes } from "../typegen.js";
// after
import { generateFormTypes } from "../forms/typegen.js";
```
(line 4's `from "../index.js"` is unchanged — index stayed put.)

- [ ] **Step 4: Fix test and bench imports**

| file:line | before | after |
|---|---|---|
| `tests/fillqueue.test.ts:2` | `"../src/fields.ts"` | `"../src/forms/fields.ts"` |
| `tests/listbox.test.ts:2` | `"../src/fields.ts"` | `"../src/forms/fields.ts"` |
| `tests/listbox.test.ts:3` | `"../src/errors.ts"` | `"../src/core/errors.ts"` |
| `tests/listbox.test.ts:4` | `"../src/form.ts"` | `"../src/forms/form.ts"` |
| `tests/typegen.test.ts:4` | `"../src/form.ts"` | `"../src/forms/form.ts"` |
| `tests/typegen.test.ts:5` | `"../src/typegen.ts"` | `"../src/forms/typegen.ts"` |
| `tests/typegen.test.ts:10` | `readFileSync(join(import.meta.dir, "../src/typegen.ts"), "utf8")` | `readFileSync(join(import.meta.dir, "../src/forms/typegen.ts"), "utf8")` |
| `tests/text-maxlen.test.ts:4` | `"../src/fields.ts"` | `"../src/forms/fields.ts"` |
| `tests/text-maxlen.test.ts:5` | `"../src/errors.ts"` | `"../src/core/errors.ts"` |
| `tests/text-maxlen.test.ts:7` | `"../src/form.ts"` | `"../src/forms/form.ts"` |
| `bench/bench.ts:16` | `"../src/form.ts"` | `"../src/forms/form.ts"` |

Then confirm nothing else references the old paths:

```bash
grep -rn 'src/form\.ts\|src/fields\.ts\|src/errors\.ts\|src/typegen\.ts\|src/schema\.ts\|"\./form\.js"\|"\./fields\.js"\|"\./errors\.js"\|"\./schema\.js"\|"\./typegen\.js"\|"\./wasm' src tests scripts bench examples --include='*.ts'
```

Expected: no output.

- [ ] **Step 5: Verify zero behavior change**

```bash
bun run typecheck
bun test
bun run build:js
```

Expected: typecheck clean, all tests pass, build emits `dist/core/*` and `dist/forms/*` alongside `dist/index.js`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move sources into src/core and src/forms"
```

---

### Task 3: Add ./forms subpath export

**Files:**
- Create: `src/forms/index.ts`
- Modify: `package.json` (exports map)
- Test: `tests/forms-entry.test.ts`

- [ ] **Step 1: Write the failing test**

Create `tests/forms-entry.test.ts`:

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/forms-entry.test.ts`
Expected: FAIL — cannot resolve `../src/forms/index.ts`.

- [ ] **Step 3: Create the barrel**

Create `src/forms/index.ts`:

```ts
// Runtime-neutral subpath entry: the AcroForm API without PdfDocument or any
// WASM import, so it loads identically under Node and browser bundlers.
// PdfDocument comes from the package root (or /browser) entry.
export { PdfForm } from "./form.js";
export type { FieldInfo, FieldType, FieldWidget } from "./form.js";
export {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./fields.js";
export {
  PdfError,
  UnknownFieldError,
  FieldTypeError,
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
  PdfCoreError,
} from "../core/errors.js";
export { generateFormTypes } from "./typegen.js";
export type { GenerateFormTypesOptions } from "./typegen.js";
export type {
  FieldMeta,
  FormSchema,
  FieldNameOf,
  NameOfType,
  OptionsOf,
  TypedPdfForm,
} from "./schema.js";
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun test tests/forms-entry.test.ts`
Expected: PASS (both tests).

- [ ] **Step 5: Add the subpath to package.json**

In `package.json`, add to `exports` after the `"./browser"` entry (keep `./typegen` last):

```json
    "./forms": {
      "types": "./dist/forms/index.d.ts",
      "import": "./dist/forms/index.js"
    },
```

- [ ] **Step 6: Verify the built entry resolves**

```bash
bun run build:js
node --input-type=module -e "import('./dist/forms/index.js').then(m => { if (!m.PdfForm) throw new Error('PdfForm missing'); console.log('forms entry ok'); })"
```

Expected: `forms entry ok`.

- [ ] **Step 7: Full suite**

```bash
bun run typecheck
bun test
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/forms/index.ts package.json tests/forms-entry.test.ts
git commit -m "feat: add ./forms subpath export"
```

---

### Task 4: Final verification sweep

**Files:** none (verification only)

- [ ] **Step 1: Browser entry smoke + CLI test**

```bash
bun run test:browser-entry
bun test tests/cli-bin.test.ts
```

Expected: both pass — these exercise `index.browser.ts` resolution and the `bin/generate-types` import chain end to end. (`test:browser-entry` runs a full `bun run build`, which needs `wasm-pack`; if it is not installed locally, run `bun run build:js && bun run scripts/browser-entry-smoke.ts` instead, since no Rust changed.)

- [ ] **Step 2: Rust untouched check**

```bash
git diff --stat master -- crates/ | cat
```

Expected: no output (M19 touches no Rust).

- [ ] **Step 3: Commit (only if anything was fixed)**

If steps 1–2 surfaced fixes, commit them with a message describing the fix; otherwise nothing to commit.
