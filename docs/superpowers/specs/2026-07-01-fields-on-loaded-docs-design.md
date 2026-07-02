# Add form fields to loaded PDFs — Design

**Date:** 2026-07-01
**Status:** Design — pending user approval
**Scope:** Part 2 of the unified-getForm work (2026-06-30). Lets `createForm()`
add brand-new AcroForm fields to a document opened with `PdfDocument.load()`,
not just one built with `PdfDocument.create()`. Full field-type parity with the
create path in a single slice.

## Summary

Today `createForm()` throws in load mode — you can read and fill an existing
form on a loaded PDF, but you cannot add a new field to one. This slice removes
that gate. On a loaded document, `createForm()` returns the **same
`FormBuilder`** used for created documents; the accumulated field definitions
are injected into the loaded PDF (parse → mutate → serialize) the first time
`getForm()` (or `save()`) runs, exactly mirroring the existing
`materializeCreatedForm()` pattern for created docs.

The work is almost entirely a **refactor-and-reuse**: the per-field/widget/
appearance construction already exists in `create.rs::build_fields_and_acroform`,
and the load-mode "add a new annotation object to an existing page and clone the
existing AcroForm" machinery already exists in `draw.rs` and `fill.rs`. The new
code is the bridge between them — a thin Rust module that runs the existing field
builder against an `IncrementalDocument` and merges into any existing AcroForm.

This closes the primary remaining pdf-lib parity gap
(`form.createTextField` / `createCheckBox` / … on a loaded PDF).

## Goals

- `doc.createForm()` on a `PdfDocument.load()` document returns a `FormBuilder`
  (same surface as create mode) instead of throwing.
- **Full field-type parity in this slice:** text (plain, multiline, comb,
  password), checkbox, radio group, dropdown, list box, and signature — every
  type the builder supports on created docs.
- **Pre-built appearance streams** for every injected field (`/AP`), keeping the
  document's `/NeedAppearances` untouched — same fidelity guarantee as the create
  path; rendering never depends on a viewer regenerating appearances.
- Works whether or not the loaded PDF already has an `/AcroForm`: merge into an
  existing one, or create and attach a new one.
- **Reject name collisions** with fields already present in the document, with a
  clear error.
- Reuse the existing `getForm()` surface: after injection the fields are read
  back through the normal load-mode `PdfForm`, so fill/flatten/read work with no
  new API.
- **Hot path untouched:** `apply_all` and the load→mutate→save pipeline are
  unchanged; a loaded doc that never calls `createForm()` behaves and performs
  exactly as today (honors the perf hot-path preference).
- Create-path output stays **byte-identical** after the shared refactor.
- No public API break: `createForm()`, `getForm()`, and `FormBuilder` signatures
  are unchanged.

## Non-goals (this slice)

- **Filling embedded-font (Type0) fields on loaded docs.** Creating an
  embedded-font field on a loaded doc renders correctly at build time (pre-built
  `/AP`), but calling `setText()` on it and saving still hits the existing
  `fill.rs` Type0 guard. That "scope-B" embedded-font fill seam is unchanged and
  remains a separate follow-up.
- **Folding field creation into `apply_all`.** Injection is a dedicated pass
  (see Architecture), not a new `ApplyPlan` phase; keeping the batched save path
  free of a build concern is deliberate.
- **Coordinating page-structure ops with field creation** in the same session
  beyond what falls out of the normal flush ordering (see Edge cases).
- **Rotation-adjusted field placement.** `x`/`y` are in the page's default user
  space, matching `drawText` and the create path.
- **Auto-threading the `FormBuilder` schema** into `getForm()`'s type parameter
  (typed narrowing). Unchanged from the create-doc decision; deferred.

## Background / current state

### The create-vs-load gate (TypeScript)

- `src/core/document.ts` `createForm()` (≈lines 530–538) throws in load mode:
  > *createForm() is only available on documents created with PdfDocument.create(). Adding new form fields to a loaded PDF is not yet supported …*
- Field defs accumulate in `this.fieldDefs: FieldDef[]` / `this.fieldNames:
  Set<string>` (document state), consumed **only** by the create path
  (`buildCreatedBytes()` → `wasm.createDocument(..., JSON.stringify(fieldDefs))`).
- `getForm()` (≈608–640) calls `materializeCreatedForm()` for a created,
  not-yet-sealed doc, then lazily builds `this.form = new PdfForm(this.bytes,
  readFields)`. It always reads fields out of `this.bytes`.
- `materializeCreatedForm()` (≈647–659): builds real bytes via
  `buildCreatedBytes()`, reassigns `this.bytes`, seals the draw queue, sets
  `sealed = true`, and hands off to the load-mode save pipeline. **This is the
  pattern the loaded-doc flush mirrors.**
- `FormBuilder` (`src/generate/form-builder.ts`) is a pure in-memory accumulator
  over `this.fieldDefs`/`this.fieldNames` (by reference); it makes no WASM call
  itself. The `FieldDef` union (text/checkBox/radioGroup/choice/signature) and
  all `addX` methods are reusable as-is in load mode.

### The field builder (Rust, create path)

- `crates/core/src/create.rs` `build_fields_and_acroform(doc, fields, page_ids,
  embedded_fonts, font_descs, fonts) -> Result<Option<ObjectId>>`
  (≈1677–2230) builds, for each `FieldDef`: the widget/field dict (`/Type
  Annot`, `/Subtype Widget`, `/FT`, `/T`, `/Rect`, `/DA`, `/Q`, `/V`/`/DV`,
  `/Ff` flags, `/MK`/`/BS`, tooltip, `/P`, `/MaxLen`), a pre-built appearance
  XObject (`/AP`) via `appearance::build_appearance_xobject`
  (button on/off streams for checkboxes/radios), appends each widget id to its
  page's `/Annots` (≈2188–2217), builds the `/DR/Font` registry, and creates the
  `/AcroForm` dict (`/Fields`, `/DR`, `/DA "/Helv 0 Tf 0 g"`, `/NeedAppearances
  false`, ≈2219–2228). Radio groups build a parent field with per-option `Kids`
  widgets sharing `/Parent`.
- **Alias coupling:** `/DA` strings and the AP resource name use **fixed**
  aliases (`Helv`, `BPF<n>`) resolved from a fresh `font_registry`. For a
  from-scratch doc these never collide. Crucially,
  `build_appearance_xobject(content, w, h, font_alias, font_ref)` writes the
  font reference into the **appearance stream's own `/Resources`**, so the
  rendered appearance is self-contained and does **not** depend on the AcroForm
  `/DR`. The `/DR` + `/DA` matter only to viewers that regenerate appearances
  (i.e. when `/NeedAppearances` is true) or to form editors.

### Load-mode mutation machinery (Rust) — already exists

All load-mode mutators share a **load → resolve → `IncrementalDocument::
create_from` → copy-on-write mutate → `inc.save_to`** shape (`lopdf`
incremental update):

- `crates/core/src/draw.rs`: `apply_draw_ops_json` already adds brand-new
  annotation dicts to existing pages. `append_annot_to_page(inc, page_id,
  annot_id)` (≈1878–1909) handles inline-array / indirect-array-clone / absent
  `/Annots`. The `Link` op path (≈1697–1730) is a direct template for adding a
  Widget annotation. Objects are added with `inc.new_document.add_object(...)`
  and pages cloned via `inc.opt_clone_object_to_new_document(page_id)`.
- `crates/core/src/fill.rs`: `clear_need_appearances(inc)` (≈1108–1136) is the
  exact **clone-and-edit-the-existing-AcroForm** pattern, handling both the
  indirect-`/AcroForm`-reference and inline-`/AcroForm`-in-catalog cases.
- `crates/core/src/flatten.rs`: `filter_fields` (≈233–275) clones the AcroForm
  and rewrites `/Fields` — the inverse operation, same COW idiom.
- `crates/core/src/pageops.rs`: `rebuild_acroform` (≈120–246) assembles an
  `/AcroForm` over an existing (merged) doc, merging `/DR/Font` across sources
  and prefixing colliding partial names — precedent for both `/DR` merge and
  name-collision handling.
- `crates/core/src/doc_io.rs`: `load_pdf(data)` is the shared parse entry and
  rejects encrypted docs; every mutator flows through it.
- `crates/core/src/lib.rs`: existing `#[wasm_bindgen]` exports. There is no
  field-injection export today.

## Architecture

### Data flow (TypeScript)

```
PdfDocument.load(bytes)              // mode = "load"
  → createForm()                     // NEW: returns FormBuilder (no throw)
      → addTextField / addCheckBox / … // push FieldDef onto this.fieldDefs
  → getForm()  (or save())           // first call with pending fieldDefs:
      → injectPendingFields()        // NEW, mirrors materializeCreatedForm()
          → wasm.injectFields(this.bytes, JSON.stringify(fieldDefs), fonts, fontsJson)
          → this.bytes = injected    // swap in the new bytes
          → clear pending fieldDefs, mark form-built
      → new PdfForm(this.bytes, readFields)   // normal load-mode form
  → form.getTextField("total").setText("42")  // normal fill
  → save()                           // normal applyAll path (unchanged)
```

- **`createForm()` in load mode** drops the throw and returns `new
  FormBuilder(this.fieldDefs, this.fieldNames)` (identical to create mode). It
  throws only if the form has already been built — tracked by the existing
  `this.form` handle being set (the same signal `getForm()` already uses for its
  lazy-construct guard); no new state flag is needed.
- **`injectPendingFields()`** (new private method on the document) is the
  loaded-doc analogue of `materializeCreatedForm()`: if `mode === "load"` and
  there are pending `fieldDefs`, call the new WASM export, reassign `this.bytes`,
  clear the pending defs, and record that the form is now built. Invoked from
  `getForm()` (before constructing `PdfForm`) and from `save()` (if `getForm()`
  was never called but fields are pending). Embedded fonts already registered on
  the doc are passed through the same `fonts`/`fontsJson` blobs the create path
  uses.
- **Ordering rule (consistent with created docs):** all `createForm()`
  field-adds must occur **before** the first `getForm()`. Calling `createForm()`
  after the form is built throws (`FormSealedError` or an equivalently clear
  error). This does **not** restrict page ops or fill, which continue to work on
  loaded docs as today.
- **No new public types.** `getForm<S>()` typing and `FormBuilder` are unchanged.

### Rust core

New export in `crates/core/src/lib.rs`:

```rust
/// Inject new AcroForm fields (JSON array of field defs, same schema as
/// create_document's fields_json) into a loaded PDF and return new bytes.
/// `fonts` / `fonts_json` carry embedded fonts referenced by fields.
#[wasm_bindgen]
pub fn inject_fields(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
) -> Result<Vec<u8>, JsError>;
```

Implemented in a new module `crates/core/src/inject.rs`:

1. `load_pdf(data)` (rejects encrypted) → parse existing field names by walking
   `/AcroForm/Fields` → each field's `/T`.
2. **Collision check:** parse `fields_json` into `Vec<FieldDef>`; if any incoming
   `name` matches an existing field name, return
   `field name '<name>' already exists in this document`. (Within-batch
   duplicates are already rejected upstream by the builder / `validate_create`.)
3. `IncrementalDocument::create_from(data.to_vec(), doc)`.
4. **Resolve target pages:** `inc.get_prev_documents().get_pages()` sorted by
   page index (as `draw.rs` does); a field's `page` indexes this list. Bad page
   index → error.
5. **Build fields** by reusing the create path's per-field construction,
   refactored to run against the `IncrementalDocument` (add objects to
   `inc.new_document`, append widgets to real pages via
   `append_annot_to_page`). Embedded fonts are built with the existing
   `build_embedded_font` engine, exactly as the create path does.
6. **AcroForm merge** (using the `clear_need_appearances` clone pattern for both
   indirect-ref and inline-in-catalog forms):
   - **No existing `/AcroForm`:** create one (`/Fields`, `/DR`, `/DA`,
     leave `/NeedAppearances` unset) and attach it to the catalog.
   - **Existing `/AcroForm`:** clone it; **append** the new field ids to
     `/Fields`; merge the injected fonts into `/DR/Font` under **freshly
     uniquified alias names** that do not collide with existing `/DR/Font` keys;
     preserve the doc's existing `/DA` and `/NeedAppearances` as-is.
   - We never set `/NeedAppearances` ourselves (appearances are pre-built). If
     the doc already had it `true`, it stays `true`.
7. `inc.save_to(...)` → return bytes.

### Shared refactor: parameterize the font alias

`build_fields_and_acroform` currently bakes fixed aliases (`Helv`, `BPF<n>`)
into each field's `/DA` and AP resource name. To let the load path avoid
clobbering an existing `/DR`, the alias becomes an **input** to the per-field
build:

- Extract the per-field construction (or add an alias-map parameter) so both the
  create path and the inject path share it.
- **Create path** passes its current fixed aliases → output stays
  byte-identical (guarded by a regression test).
- **Inject path** passes uniquified aliases when merging into an existing `/DR`,
  and rewrites each injected field's `/DA` + AP resource name to match.

Because the AP stream is self-contained (font ref in its own `/Resources`), even
if the alias differs from the doc's convention the field still renders correctly;
the uniquified alias keeps `/DR`-based editing/regeneration correct too.

## Error handling

- **Name collision** with an existing field → error (see above); no partial
  mutation (the check runs before any object is added).
- **Bad page index** (field targets a page that doesn't exist) → error, matching
  the create path's `rejects_field_bad_page`.
- **Encrypted document** → the existing `load_pdf` `ENCRYPTED:`/`PASSWORD:`
  errors surface (no field injection on encrypted docs).
- **`createForm()` after `getForm()`** → throws (form already built).
- Existing builder/`validate_create` guards (comb+embedded font, choice+embedded
  font, duplicate names within the batch, unknown font) apply unchanged, since
  the inject path parses the same `FieldDef` schema.
- Errors are converted through the existing `toPdfError` path in `document.ts`.

## Edge cases (documented, accepted)

- **Coordinates:** `x`/`y` are in the target page's default user space (points,
  origin bottom-left), matching `drawText`/create. On a page with `/Rotate`,
  positions are not visually rotation-adjusted (same as existing draw ops).
- **Embedded-font fill:** creating an embedded-font field on a loaded doc works;
  *filling* it via `setText()` still hits the `fill.rs` Type0 guard — unchanged,
  separate follow-up.
- **Page-structure ops + field creation in one session:** field positions
  reference page indices in the document state at flush time; mixing queued
  insert/remove/move with new-field creation in the same session is not specially
  coordinated. Recommendation: flush (call `getForm()`/`save()`) between a page
  op and dependent field creation.

## Testing

**Rust unit tests (`crates/core/src/inject.rs`):**
- Inject each field type (text plain/multiline/comb/password, checkbox, radio,
  dropdown, listbox, signature) into a loaded fixture; assert the catalog
  `/AcroForm/Fields` grew, the widget landed on the target page's `/Annots`,
  `/AP` is present, and `/NeedAppearances` is unchanged.
- Inject into a PDF with **no** `/AcroForm` → a new `/AcroForm` is created and
  attached to the catalog.
- Inject into a PDF **with** an existing `/AcroForm` → existing fields survive,
  new fields appended, `/DR/Font` gains uniquified aliases without dropping
  existing ones.
- Name collision with an existing field → error, document unmodified.
- Bad page index → error.
- qpdf structural validation on the output bytes.

**Rust regression:**
- Helvetica-only (and multi-font) create output is **byte-identical** after the
  alias-parameterization refactor.

**TypeScript integration (`tests/`):**
- `load → createForm → add each field type → getForm → readFields sees them →
  fill → save → reload → values round-trip`.
- `createForm()` after `getForm()` throws.
- A loaded doc that never calls `createForm()` is byte-identical / behavior-
  identical to today (no regression on the hot path).
- Injecting into a real fixture that already has a form (`FICHA`) keeps the
  pre-existing fields fillable.

## Rollout

- Version bump: **minor** (new backward-compatible behavior). Adjust if another
  milestone ships first.
- Update `createForm()` docs / README limitations (drop "not supported on loaded
  PDFs"); update CHANGELOG.
- Related: [[getform-created-docs-architecture]] (materialize-and-seal pattern
  this mirrors), [[embedded-font-form-fields-architecture]] (the Type0 fill seam
  that remains out of scope).

## Future work

- Embedded-font **fill** on loaded/materialized fields (replaces the `fill.rs`
  Type0 guard) — the natural next slice, unblocks `setText()` on injected
  embedded-font fields.
- Rotation-aware field placement, if demand appears.
