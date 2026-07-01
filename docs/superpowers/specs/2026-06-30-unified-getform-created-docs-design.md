# Unified `getForm()` on created documents — Design

**Date:** 2026-06-30
**Status:** Approved (design)

## Summary

Make `getForm()` work on documents created with `PdfDocument.create()`, so a
user can build form fields with `createForm()`, then read, fill, and flatten
those fields in the **same session** — without the current save-and-reload
round-trip. This closes the largest ergonomic gap versus pdf-lib, whose
`form.createTextField(...)` → `getForm()` → `setText(...)` flow works in one
session.

The feature is create-mode only and strictly opt-in: a created document that is
never passed through `getForm()` pays no extra cost.

## Goals

- `getForm()` on a created document returns a working `PdfForm` for reading,
  filling, and flattening fields that were added via `createForm()`.
- **Full round-trip fidelity:** values read back (generated appearance streams,
  resolved `/DA`, widget rects) match a save-then-reload of the same document,
  because they are parsed from the real generated output — not a re-implemented
  appearance engine.
- **Zero new Rust and no duplicated appearance logic** — reuse the existing
  `createDocument`, `readFields`, and load-mode fill/flatten/save pipeline.
- **Load→mutate→save hot path untouched.** No behavior or performance change for
  loaded documents.
- Backward compatible: no public API signature changes.

## Non-goals (this spec)

- **Adding new fields to a *loaded* document** (pdf-lib's `form.createTextField`
  on a `load()`ed PDF). This is a separate, larger milestone requiring Rust work
  to inject an AcroForm into an already-parsed PDF (create `/AcroForm` if absent,
  append field + widget dicts to pages, merge `/DR`, set `/NeedAppearances`,
  resolve name collisions with existing fields). It would reuse this `getForm()`
  surface, so nothing here blocks it.
- **Auto-threading the `FormBuilder` schema** into `getForm()`'s type parameter
  (typed narrowing for created docs). Deferred; would complicate the public
  document type, which we keep single and stable.
- **An ESLint plugin** to statically enforce the build-before-`getForm()`
  ordering. Possible as a separate, optional package (`eslint-plugin-better-pdf`),
  but it can only catch local/same-function misuse — the runtime seal remains the
  real enforcement. Not designed here.

## Background — the current split

- **Create mode:** `createForm()` returns a `FormBuilder` that accumulates plain
  JS `fieldDefs`. Fields become real PDF bytes only at `save()`, via a single
  `wasm.createDocument(opsJson, images, fonts, fontsJson, fieldDefsJson)` pass.
  There are no intermediate bytes.
- **Load mode:** `getForm()` constructs `new PdfForm(bytes, readFields)`, which
  parses fields from the existing bytes. Fill/flatten operations queue on the
  form and apply at `save()` through `applyAll`.
- **The gap:** a created document has field *definitions* in JS but no bytes, so
  `readFields` has nothing to parse — `getForm()` throws in create mode.

Every field type already supports setting an **initial value at build time**
(`value`/`defaultValue` for text, `checked`/`defaultChecked` for checkbox,
`selected`/`defaultSelected` for radio/dropdown/listbox). So `getForm()` on a
created doc is **not** needed to set initial values. Its marginal value is:
filling with data computed later (fill loops decoupled from layout), reading /
listing fields, and matching the pdf-lib mental model.

## Approach — materialize on `getForm()`

The first time `getForm()` is called on a created document, internally run the
existing create-save pass to produce real PDF bytes, then reopen those bytes as a
load-backed form. In effect:

> `getForm()` on a created doc = internal `save()` → reopen the result as a
> loaded document.

Fidelity is a byproduct: you parse the *actual* generated PDF, so appearances,
`/DA`, and widget geometry are exact.

### Design invariant: materialization is lazy and opt-in

Materialization lives **inside `getForm()`, never inside `save()`**.

- Create → `save()`, no `getForm()`: normal create path, a single
  `createDocument` pass. Zero extra cost. (Honors the perf hot-path preference.)
- Create → `getForm()` → `save()`: `getForm()` runs one `createDocument` pass
  (materialize + cache); `save()` then takes the load-mode `applyAll` path on the
  cached bytes. Exactly one extra pass, only because `getForm()` was called.

## State model & seal enforcement

Add one internal flag, `sealed` (default `false`). No change to `mode`, no new
public type — the public `PdfDocument` type stays single and stable.

### Transition (inside `getForm()`, create mode, first call)

1. Run the create-save pass → `materializedBytes` (consumes the draw queue,
   `fieldDefs`, and any dirty metadata/outline — identical to what `save()` would
   have produced at that moment).
2. `this.bytes = materializedBytes`
3. `this.form = new PdfForm(materializedBytes, readFields)`
4. `this.sealed = true`; clear `meta.dirty` and the consumed outline so they are
   not re-applied on the subsequent load-path save.
5. Return `this.form`. Later `getForm()` calls return the cached form with **no**
   re-materialization.

### Save routing

The create-path guard becomes `if (mode === "create" && !sealed)` →
`createDocument`. Once sealed, `save()` falls through to the existing load-mode
`applyAll` path (fill / flatten / metadata / outline) against `materializedBytes`.
This is what prevents any double-build.

### Enforcement — after `getForm()` on a created doc

| Operation | Behavior |
|---|---|
| `createForm()`, `addPage()`, `insertPage/removePage/movePage()` | **throw** (seal message) |
| Any draw on a `PdfPage` handle (`drawText`, `drawImage`, …) | **throw** (seal message) |
| `getForm()` again | returns cached `PdfForm` (no re-materialization) |
| form fill / read / `flatten()` | allowed (load pipeline) |
| `setTitle/setAuthor/…`, `setOutline()` | allowed (applied via load path on save) |
| `save()` | allowed (load path) |

Draws are guarded at the **draw-queue entry point** (the queue is marked sealed
at transition), so already-handed-out `PdfPage` handles throw rather than
silently no-op'ing.

## API surface & error messages

No public method or signature changes. `getForm(): PdfForm` and
`getForm<S extends FormSchema>(): TypedPdfForm<S>` are unchanged. A created doc
returns a plain (untyped) `PdfForm`; passing a generated schema type argument
still type-checks.

Three error-message sites:

1. **`createForm()` on a loaded doc** (improved DX):
   > `createForm() is only available on documents created with PdfDocument.create(). Adding new form fields to a loaded PDF is not yet supported — to build and fill a form, create a document, add fields with createForm(), then call getForm() to read or fill them.`

2. **`getForm()` on a created doc:** the old throw is **removed** — it now works.

3. **Seal violation** (any sealed operation above):
   > `content creation is sealed after getForm() on a created document; add all fields, pages, and drawings before calling getForm().`

## Loaded-document behavior — unchanged

For a loaded PDF, `mode === "load"`, so the materialize-and-seal branch is never
entered and `sealed` stays `false`. `getForm()` runs the same
`if (!this.form) this.form = new PdfForm(bytes, readFields)` as today, the seal
table never applies, and save routing is unaffected (`mode === "load"` skips the
create branch). Fully backward compatible.

## Edge cases

- **Empty created doc** (no fields): `getForm()` materializes a valid PDF,
  `readFields` returns `[]`, an empty form is returned. Matches pdf-lib.
- **Pending draws, no fields:** materialization includes the draws; the form is
  empty; drawn content survives to output.
- **`getForm()` twice:** same instance, single materialization.
- **Metadata/outline set after `getForm()`:** applied via the load path on save.

## Testing

Extend the existing form-fixture suite.

**Happy path (created-doc unification):**
- Build text/checkbox/radio/dropdown/listbox via `createForm()` → `getForm()` →
  `getFields()` returns all fields with correct names, types, and build-time
  initial values.
- `getTextField(name).setText(...)` → `save()` → reload → value round-trips.
- Full-fidelity read-back: after `getForm()`, appearance streams / resolved
  `/DA` / widget rects match a save-then-reload baseline.
- Fill a value not known at build time → save → reload → assert (core use case).
- `flatten()` a created field → save → reload → baked into page content, no
  longer interactive.

**Seal enforcement:**
- After `getForm()`, each of `createForm()`, `addPage()`,
  `insertPage/removePage/movePage()`, and a draw on a prior `PdfPage` handle
  throws the seal message.
- `getForm()` twice → same instance, no second materialization.
- `setTitle` / `setOutline()` after `getForm()` still apply on save.

**Perf invariant:**
- Create → `save()` without `getForm()` runs exactly one `createDocument` pass
  (no materialization) — proves the fee is opt-in.

**Edge cases:**
- Empty created doc → empty form, `getFields()` === `[]`.
- Created doc with pending draws but no fields → draws survive, form empty.

**Regression / errors:**
- `createForm()` on a loaded doc throws the improved message.
- Existing loaded-doc `getForm()` tests stay green (no behavior change).

## Future work

- Add new fields to loaded documents (pdf-lib parity, part 2).
- Auto-thread the `FormBuilder` schema into `getForm()` typed narrowing.
- Optional `eslint-plugin-better-pdf` for build-before-`getForm()` ordering.
