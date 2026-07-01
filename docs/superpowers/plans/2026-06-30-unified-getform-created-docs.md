# Unified `getForm()` on Created Documents — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `getForm()` work on documents created with `PdfDocument.create()`, so fields built via `createForm()` can be read, filled, and flattened in the same session without a manual save-and-reload.

**Architecture:** The first `getForm()` call on a created document *materializes* it — runs the existing `createDocument` pass to produce real PDF bytes, swaps those bytes in, and constructs the normal load-mode `PdfForm` over them. The document is then *sealed*: no more fields, pages, or draws. `save()` afterward runs the load-mode pipeline on the materialized bytes. Fidelity is exact because the form is parsed from the real generated output. This is a TypeScript-only change — no Rust/WASM changes.

**Tech Stack:** TypeScript, Bun test runner (`bun test`), existing WASM core (`createDocument`, `readFields`, `applyAll` already exist).

## Global Constraints

- **No public `PdfDocument` type split** — add internal state only; `getForm()` / `createForm()` signatures unchanged. (Verbatim project rule: "don't split the public `PdfDocument` type".)
- **Materialization is opt-in** — it lives inside `getForm()`, never `save()`. A created doc that never calls `getForm()` pays exactly one `createDocument` pass at `save()`, as today. The load→mutate→save hot path is untouched.
- **CI gates clippy + tests, not rustfmt** — this plan touches no Rust. Run `bun test` for every task.
- **Follow existing patterns** — error classes live in `src/core/errors.ts` and extend `PdfError`; exported from `src/exports-common.ts`.

---

### Task 1: Add `FormSealedError`

**Files:**
- Modify: `src/core/errors.ts` (append new class after `InvalidRotationError`, ~line 118)
- Modify: `src/exports-common.ts` (add to the error re-export block, ~line 46-57)
- Test: `tests/errors.test.ts`

**Interfaces:**
- Produces: `class FormSealedError extends PdfError` — default message
  `"content creation is sealed after getForm() on a created document; add all fields, pages, and drawings before calling getForm()."`; exported from the package root.

- [ ] **Step 1: Write the failing test**

Append to `tests/errors.test.ts`:

```ts
import { FormSealedError, PdfError } from "../src/index.ts";

test("FormSealedError has instructive message and name", () => {
  const err = new FormSealedError();
  expect(err).toBeInstanceOf(PdfError);
  expect(err.name).toBe("FormSealedError");
  expect(err.message).toContain("sealed after getForm()");
});
```

(If `tests/errors.test.ts` lacks a `test`/`expect` import, add `import { expect, test } from "bun:test";` at the top — check first, don't duplicate.)

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/errors.test.ts`
Expected: FAIL — `FormSealedError` is not exported / undefined.

- [ ] **Step 3: Add the error class**

In `src/core/errors.ts`, after the `InvalidRotationError` class (before `toInvalidImageError`):

```ts
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
```

In `src/exports-common.ts`, add `FormSealedError,` to the error re-export list from `./core/errors.js` (the block containing `PdfError, UnknownFieldError, ...`):

```ts
  PdfError,
  FormSealedError,
  UnknownFieldError,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun test tests/errors.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/core/errors.ts src/exports-common.ts tests/errors.test.ts
git commit -m "feat(errors): add FormSealedError"
```

---

### Task 2: Seal the `DrawQueue`

**Files:**
- Modify: `src/generate/draw-queue.ts` (class `DrawQueue`, ~line 140-321)
- Test: `tests/draw-queue-seal.test.ts` (create)

**Interfaces:**
- Consumes: `FormSealedError` from Task 1.
- Produces: `DrawQueue.seal(): void` — after it is called, every `push*` method
  (`pushText`, `pushImage`, `pushPage`, `pushLine`, `pushRectangle`,
  `pushEllipse`, `pushSetRotation`, `pushSetMediaBox`, `pushLink`, `pushPath`,
  `pushAddPage`, `pushMetadata`, `pushOutline`) throws `FormSealedError`.
  Read/serialize methods (`toCreatePayload`, `toDrawPayload`, `length`,
  `registerFont`) still work.

- [ ] **Step 1: Write the failing test**

Create `tests/draw-queue-seal.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { DrawQueue } from "../src/generate/draw-queue.ts";
import { FormSealedError } from "../src/index.ts";

describe("DrawQueue.seal", () => {
  test("push after seal throws FormSealedError", () => {
    const q = new DrawQueue();
    q.pushAddPage(100, 200); // ok before seal
    q.seal();
    expect(() =>
      q.pushText(0, "x", { x: 0, y: 0, size: 12, font: "Helvetica", color: [0, 0, 0] }),
    ).toThrow(FormSealedError);
    expect(() => q.pushAddPage(1, 1)).toThrow(FormSealedError);
  });

  test("serialization still works after seal", () => {
    const q = new DrawQueue();
    q.pushAddPage(100, 200);
    q.seal();
    const payload = q.toCreatePayload();
    expect(payload.opsJson).toContain("addPage");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/draw-queue-seal.test.ts`
Expected: FAIL — `q.seal is not a function`.

- [ ] **Step 3: Implement seal + guard**

In `src/generate/draw-queue.ts`, add the import at the top (after the existing imports):

```ts
import { FormSealedError } from "../core/errors.js";
```

Inside `class DrawQueue`, add a field next to the other private fields (after `private outlineOp: ...`):

```ts
  private sealed = false;

  /** After this, every push throws — used when a created doc is materialized. */
  seal(): void {
    this.sealed = true;
  }

  private assertOpen(): void {
    if (this.sealed) throw new FormSealedError();
  }
```

Add `this.assertOpen();` as the **first line** of each push method: `pushText`, `pushAddPage`, `pushMetadata`, `pushOutline`, `pushImage`, `pushPage`, `pushLine`, `pushRectangle`, `pushEllipse`, `pushSetRotation`, `pushSetMediaBox`, `pushLink`, `pushPath`.

Example for `pushText`:

```ts
  pushText(
    page: number,
    text: string,
    opts: { /* unchanged */ },
  ): void {
    this.assertOpen();
    this.drawOps.push({ /* unchanged */ });
  }
```

Example for `pushLine` (and the identical one-liner pattern for `pushRectangle`, `pushEllipse`, `pushLink`, `pushPath`):

```ts
  pushLine(op: LineOp): void {
    this.assertOpen();
    this.drawOps.push(op);
  }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun test tests/draw-queue-seal.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/generate/draw-queue.ts tests/draw-queue-seal.test.ts
git commit -m "feat(draw-queue): add seal() to freeze pushes after materialization"
```

---

### Task 3: Materialize on `getForm()` + save routing

**Files:**
- Modify: `src/core/metadata-state.ts` (add `clearDirty`, ~line 28)
- Modify: `src/core/document.ts` — constructor field (`bytes`, line 155), `save()` (lines 182-249), `getForm()` (lines 600-614); add private helpers.
- Test: `tests/created-form-getform.test.ts` (create)

**Interfaces:**
- Consumes: `DrawQueue.seal()` (Task 2), `FormSealedError` (Task 1).
- Produces:
  - `MetadataState.clearDirty(): void` — resets the dirty flag without discarding values.
  - Internal `private sealed = false` on `PdfDocumentBase`.
  - Internal `private buildCreatedBytes(): Uint8Array` — the create-save body, shared by `save()` and materialization.
  - Internal `private materializeCreatedForm(): void` — builds bytes, swaps them in, seals the draw queue, clears consumed metadata/outline.
  - Behavioral: `getForm()` works on a created doc; `save()` after `getForm()` runs the load pipeline on materialized bytes (no double build).

- [ ] **Step 1: Write the failing test**

Create `tests/created-form-getform.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes } from "../src/index.ts";

describe("getForm on created docs", () => {
  test("create -> getForm -> read fields (build-time values)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20, value: "Ada",
    });

    const form = doc.getForm();
    const field = form.getField("name");
    expect(field).toBeDefined();
    expect(field!.type).toBe("text");
    expect(field!.value).toBe("Ada");
  });

  test("create -> getForm -> setText -> save -> reload round-trips", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });

    doc.getForm().getTextField("name").setText("Grace Hopper");
    const out = await doc.save();

    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")!.value).toBe("Grace Hopper");
  });

  test("getForm returns the same instance and does not re-materialize", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });
    const a = doc.getForm();
    const b = doc.getForm();
    expect(a).toBe(b);
  });

  test("create -> save without getForm still works (opt-in materialization)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20, value: "baked",
    });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")!.value).toBe("baked");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun test tests/created-form-getform.test.ts`
Expected: FAIL — `getForm` throws `"getForm is not available on documents created with PdfDocument.create()"`.

- [ ] **Step 3: Add `MetadataState.clearDirty`**

In `src/core/metadata-state.ts`, after the `private set(...)` method (~line 28):

```ts
  /** Reset the dirty flag after values are baked into created bytes. */
  clearDirty(): void {
    this.dirtyFlag = false;
  }
```

- [ ] **Step 4: Make `bytes` reassignable + add `sealed` field**

In `src/core/document.ts`, add the `sealed` field next to the other private fields (after `private readonly appendedPages: PdfPage[] = [];`, ~line 151):

```ts
  private sealed = false;
```

Change the constructor parameter (line 155) from:

```ts
    protected readonly bytes: Uint8Array,
```

to:

```ts
    protected bytes: Uint8Array,
```

Import `FormSealedError` — extend the existing errors import in `document.ts` to include it (find the `from "./errors.js"` import and add `FormSealedError`).

- [ ] **Step 5: Extract `buildCreatedBytes` and route `save()`**

In `src/core/document.ts`, replace the create-mode branch of `save()` (lines 183-202) so it delegates to a shared helper:

```ts
  async save(): Promise<Uint8Array> {
    if (this.mode === "create" && !this.sealed) {
      try {
        return this.buildCreatedBytes();
      } catch (e) {
        throw toPdfError(e);
      }
    }

    const form = this.form;
    // ...unchanged structureOps / fast-path code follows...
```

Guard the draw block in the load path so baked create-time draws are not re-applied after materialization. Change (line 228):

```ts
    if (this.drawQueue.length > 0) {
```

to:

```ts
    if (!this.sealed && this.drawQueue.length > 0) {
```

Add the shared helper as a new private method (place it just below `save()`/`saveChained`):

```ts
  /**
   * Build the finished PDF bytes for a created document: bake queued metadata
   * and outline, then run the single-pass createDocument with all fields.
   * Shared by `save()` (create mode) and `getForm()` materialization.
   */
  private buildCreatedBytes(): Uint8Array {
    if (this.meta.dirty) {
      this.drawQueue.pushMetadata(this.meta.wire);
    }
    if (this.outlineItems !== undefined) {
      this.drawQueue.pushOutline(this.outlineItems);
    }
    const { opsJson, images, fonts, fontsJson } = this.drawQueue.toCreatePayload();
    return this.wasm.createDocument(
      opsJson,
      images,
      fonts,
      fontsJson,
      JSON.stringify(this.fieldDefs),
    );
  }
```

- [ ] **Step 6: Materialize inside `getForm()`**

In `src/core/document.ts`, replace the `getForm()` implementation body (lines 600-614) with:

```ts
  getForm(): PdfForm {
    if (this.mode === "create" && !this.sealed) {
      this.materializeCreatedForm();
    }
    if (!this.form) {
      try {
        this.form = new PdfForm(this.bytes, this.wasm.readFields);
      } catch (e) {
        throw toPdfError(e);
      }
    }
    return this.form;
  }

  /**
   * Turn a created document into a load-backed, sealed one: build real bytes,
   * swap them in, freeze the draw queue, and clear the metadata/outline that
   * are now baked into those bytes. The load-mode save pipeline takes over.
   */
  private materializeCreatedForm(): void {
    let bytes: Uint8Array;
    try {
      bytes = this.buildCreatedBytes();
    } catch (e) {
      throw toPdfError(e);
    }
    this.bytes = bytes;
    this.drawQueue.seal();
    this.sealed = true;
    this.meta.clearDirty();
    this.outlineItems = undefined;
  }
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `bun test tests/created-form-getform.test.ts`
Expected: PASS (all four tests).

- [ ] **Step 8: Run the full suite (no regressions)**

Run: `bun test`
Expected: PASS — existing loaded-doc form/save tests unaffected.

- [ ] **Step 9: Commit**

```bash
git add src/core/metadata-state.ts src/core/document.ts tests/created-form-getform.test.ts
git commit -m "feat(document): materialize created docs on getForm() for same-session read/fill"
```

---

### Task 4: Seal enforcement + improved loaded-doc `createForm()` message

**Files:**
- Modify: `src/core/document.ts` — `createForm()` (505-510), `addPage()` (411-419), `insertPage()` (437-447), `removePage()` (454-463), `movePage()` (470-482); add `assertNotSealed` helper.
- Test: `tests/created-form-getform.test.ts` (extend)

**Interfaces:**
- Consumes: `FormSealedError` (Task 1), `sealed` field (Task 3).
- Produces: after `getForm()` on a created doc, `createForm()`, `addPage()`,
  `insertPage()`, `removePage()`, `movePage()`, and any page-handle draw throw
  `FormSealedError`. `createForm()` on a loaded doc throws an instructive
  `PdfError` naming the created-doc workflow.

- [ ] **Step 1: Write the failing tests**

Append to `tests/created-form-getform.test.ts`:

```ts
import { FormSealedError, PdfError } from "../src/index.ts";

describe("seal enforcement", () => {
  async function sealedDoc() {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });
    doc.getForm(); // materialize + seal
    return { doc, page };
  }

  test("createForm after getForm throws FormSealedError", async () => {
    const { doc } = await sealedDoc();
    expect(() => doc.createForm()).toThrow(FormSealedError);
  });

  test("addPage after getForm throws FormSealedError", async () => {
    const { doc } = await sealedDoc();
    expect(() => doc.addPage(PageSizes.A4)).toThrow(FormSealedError);
  });

  test("draw on a prior page handle after getForm throws FormSealedError", async () => {
    const { page } = await sealedDoc();
    expect(() =>
      page.drawText("late", { x: 10, y: 10, size: 12 }),
    ).toThrow(FormSealedError);
  });

  test("createForm on a loaded doc throws an instructive message", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const loaded = await PdfDocument.load(await doc.save());
    expect(() => loaded.createForm()).toThrow(PdfError);
    expect(() => loaded.createForm()).toThrow(/not yet supported/);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `bun test tests/created-form-getform.test.ts`
Expected: FAIL — sealed calls currently succeed or throw the wrong error; loaded `createForm()` message lacks "not yet supported".

- [ ] **Step 3: Add `assertNotSealed` and guard the build methods**

In `src/core/document.ts`, add a private helper (near `buildPageIndexResolver`):

```ts
  private assertNotSealed(): void {
    if (this.sealed) throw new FormSealedError();
  }
```

Update `createForm()` (lines 505-510):

```ts
  createForm(): FormBuilder {
    if (this.mode !== "create") {
      throw new PdfError(
        "createForm() is only available on documents created with PdfDocument.create(). Adding new form fields to a loaded PDF is not yet supported — to build and fill a form, create a document, add fields with createForm(), then call getForm() to read or fill them.",
      );
    }
    this.assertNotSealed();
    return new FormBuilder(this.fieldDefs, this.fieldNames);
  }
```

Add `this.assertNotSealed();` as the first line of `addPage()` (before `const [width, height] = size;`), `insertPage()`, `removePage()`, and `movePage()` (before their existing `if (this.mode !== ...)` checks, so a sealed created doc reports the seal reason rather than "load only").

Example for `addPage`:

```ts
  addPage(size: PageSize = PageSizes.A4): PdfPage {
    this.assertNotSealed();
    const [width, height] = size;
    // ...unchanged...
  }
```

Example for `insertPage`:

```ts
  insertPage(index: number, size: PageSize = PageSizes.A4): void {
    this.assertNotSealed();
    if (this.mode !== "load") {
      throw new PdfError("insertPage is only available on documents opened with PdfDocument.load()");
    }
    // ...unchanged...
  }
```

(Page-handle draws are already blocked: they route through the sealed `DrawQueue` from Task 2.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `bun test tests/created-form-getform.test.ts`
Expected: PASS.

- [ ] **Step 5: Run the full suite**

Run: `bun test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/core/document.ts tests/created-form-getform.test.ts
git commit -m "feat(document): seal build ops after getForm(); improve loaded createForm() error"
```

---

### Task 5: Flatten, edge cases, metadata-after-seal, and docs

**Files:**
- Test: `tests/created-form-getform.test.ts` (extend)
- Modify: `docs/site/src/content/docs/reference/limitations.md` (the "Form creation and form reading are separate phases" bullet, ~line 28-32)

**Interfaces:**
- Consumes: all behavior from Tasks 1-4. No new production code expected —
  flatten, empty-doc, drawn-content-survival, and metadata-after-seal all ride
  the pipeline established in Task 3. If a test reveals a gap, fix it in
  `document.ts` under this task.

- [ ] **Step 1: Write the tests**

Append to `tests/created-form-getform.test.ts`:

```ts
describe("getForm on created docs — flatten & edges", () => {
  test("flatten a created field then save -> field no longer interactive", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20, value: "Locked",
    });
    const form = doc.getForm();
    form.flattenField("name");
    const out = await doc.save();

    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")).toBeUndefined();
  });

  test("empty created doc -> getForm returns an empty form", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const form = doc.getForm();
    expect(form.getFields()).toEqual([]);
  });

  test("drawn page content survives materialization", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage(PageSizes.A4);
    page.drawText("Hello", { x: 72, y: 700, size: 24 });
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 600, width: 200, height: 20,
    });
    doc.getForm(); // materialize
    const out = await doc.save();

    // Reloads and keeps both the field and (visually) the drawn text.
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")).toBeDefined();
    expect(reloaded.getPageCount()).toBe(1);
  });

  test("metadata set after getForm is applied on save", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });
    doc.getForm(); // seal
    doc.setTitle("After Seal");
    const out = await doc.save();

    const reloaded = await PdfDocument.load(out);
    const meta = await reloaded.getMetadata();
    expect(meta.title).toBe("After Seal");
  });
});
```

- [ ] **Step 2: Run the tests**

Run: `bun test tests/created-form-getform.test.ts`
Expected: PASS. If "metadata set after getForm" fails because materialization did not clear the dirty flag, re-check Task 3 Step 6 (`this.meta.clearDirty()`); if flatten fails, confirm the load-path save runs the flatten queue (it does — `form[kFlattenQueue]`).

- [ ] **Step 3: Update the limitations doc**

In `docs/site/src/content/docs/reference/limitations.md`, replace the "Form creation and form reading are separate phases" bullet (lines 28-32) with:

```markdown
- **`getForm()` works on created documents (added in this release).** After
  adding fields with the form builder
  ([Creating form fields](/better-pdf/guides/creating-form-fields/)) you can call
  `getForm()` in the **same session** to read, fill, and flatten them — no
  save-and-reload round-trip. The first `getForm()` call *materializes* the
  document (runs the create pass once and caches the bytes), after which the
  document is **sealed**: adding more fields, pages, or drawings throws
  `FormSealedError`. Do all content creation before calling `getForm()`.
  **Still unsupported:** adding brand-new AcroForm fields to a document opened
  with `PdfDocument.load()` (only reading/filling existing fields is supported
  there).
```

- [ ] **Step 4: Run the full suite one final time**

Run: `bun test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/created-form-getform.test.ts docs/site/src/content/docs/reference/limitations.md
git commit -m "test(forms): cover flatten/edges for created-doc getForm; update limitations"
```

---

## Self-Review Notes

- **Spec coverage:** materialize-on-getForm (Task 3), lazy/opt-in invariant (Task 3 Step 5 + test), seal state + enforcement table (Tasks 2 & 4), full-fidelity read via real bytes (Task 3 tests), flatten in scope (Task 5), improved loaded-doc error (Task 4), loaded-doc behavior unchanged (Task 3 Step 8 full-suite gate), edge cases empty/draws/getForm-twice/metadata-after-seal (Tasks 3 & 5). Non-goals (loaded-doc field creation, typed narrowing, ESLint plugin) intentionally have no tasks.
- **Type consistency:** `buildCreatedBytes(): Uint8Array`, `materializeCreatedForm(): void`, `assertNotSealed(): void`, `MetadataState.clearDirty(): void`, `DrawQueue.seal(): void`, `FormSealedError` — names used identically across tasks.
- **Placeholder scan:** none — every code and test step is concrete.
```
