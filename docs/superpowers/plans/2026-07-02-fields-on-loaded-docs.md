# Add form fields to loaded PDFs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `createForm()` add brand-new AcroForm fields to a document opened with `PdfDocument.load()`, with full field-type parity with the create path.

**Architecture:** Extract the create path's per-field/widget/appearance builder into a shared `build_one_field` helper (font alias parameterized), then add a new WASM export `inject_fields` that runs that shared builder against an `IncrementalDocument` (copy-on-write incremental update) and merges the new fields into any existing `/AcroForm` — or creates one. On the TypeScript side, `createForm()` stops throwing in load mode; the first `getForm()`/`save()` with pending field defs flushes them through `inject_fields` and reassigns `this.bytes`, exactly mirroring the existing `materializeCreatedForm()` pattern.

**Tech Stack:** Rust (`lopdf` 0.41, `wasm-bindgen`), TypeScript, Bun test runner, qpdf validation. Design spec: `docs/superpowers/specs/2026-07-01-fields-on-loaded-docs-design.md`.

## Global Constraints

- Rust edition and deps unchanged: `lopdf` 0.41, `wasm-bindgen`. No new crates.
- **Create-path output must stay byte-identical** after the Task 1 refactor (hard invariant; there are existing tests asserting create output — do not change object add order in the create path).
- **`apply_all` and the load→mutate→save pipeline stay untouched** — a loaded doc that never calls `createForm()` must be behavior- and byte-identical to today (perf hot-path preference).
- New WASM functions are added in exactly one place per layer: `crates/core/src/lib.rs` (export), `src/core/wasm-bindings.ts` (`RawBindings` + `makeBindings`), `src/core/document.ts` (`CoreWasm`), and the destructure lists in both `src/core/wasm.ts` and `src/core/wasm-browser.ts`.
- No public API signature changes: `createForm()`, `getForm()`, and `FormBuilder` keep their current signatures.
- Field `x`/`y` are in the target page's default user space (points, origin bottom-left); no rotation adjustment (matches `drawText`/create).
- Name collision with an existing field is a hard error; the document must not be partially mutated (check before adding any object).
- Version bump: **1.9.0** (new backward-compatible behavior).
- Build/test commands: Rust `cargo test -p better-pdf-core`; WASM rebuild `bun run build:wasm` (or the repo's documented wasm-pack build) before TS tests; TS `bun test`.

---

### Task 1: Extract shared per-field builder (`build_one_field`), byte-identical create path

**Files:**
- Modify: `crates/core/src/create.rs` — `build_fields_and_acroform` (≈1677–2230), `enum FieldDef` (≈273), and helper visibility.
- Test: `crates/core/src/create.rs` (existing `tests` module) + one new byte-identity test.

**Interfaces:**
- Produces (used by Task 2/3):
  - `pub(crate) enum FieldDef { … }` — the existing serde-tagged field-def enum, visibility widened from private to `pub(crate)`.
  - `pub(crate) struct BuiltField { pub top_field_id: ObjectId, pub widgets: Vec<(usize, ObjectId)> }` — `top_field_id` goes into `/AcroForm/Fields`; each `(page_index, widget_id)` is appended to that page's `/Annots`.
  - `pub(crate) enum FieldFont<'a> { Standard { alias: &'a str, font_ref: ObjectId }, Embedded { alias: &'a str, type0_id: ObjectId, built: &'a BuiltFont, bytes: &'a [u8] } }` — resolved font for a text/choice field.
  - `pub(crate) fn build_one_field(doc: &mut Document, field: &FieldDef, font: Option<FieldFont<'_>>) -> Result<BuiltField, String>` — adds the field/widget/AP (and radio kids) objects to `doc`; returns the field id and its widgets' target page indices. Does NOT touch page `/Annots` or the `/AcroForm`.
  - `pub(crate) fn da_font_alias(base: &str) -> Option<&'static str>` — visibility widened (already exists, ≈527).
  - `pub(crate) fn font_dict(base: &str) -> Dictionary` — visibility widened (already exists in this file).

- [ ] **Step 1: Widen visibility of the reused items**

In `crates/core/src/create.rs`, change these declarations (leave bodies unchanged):
- `enum FieldDef {` → `pub(crate) enum FieldDef {`
- `fn da_font_alias(` → `pub(crate) fn da_font_alias(`
- `fn font_dict(` → `pub(crate) fn font_dict(`

- [ ] **Step 2: Add the shared types and `build_one_field`, moving the per-field arm bodies verbatim**

Add near `build_fields_and_acroform`:

```rust
/// A built field's top-level object id plus which page each of its widget
/// annotations must be appended to. Page indices are 0-based into the caller's
/// page list (create path: fresh pages; inject path: existing pages).
pub(crate) struct BuiltField {
    pub top_field_id: ObjectId,
    pub widgets: Vec<(usize, ObjectId)>,
}

/// Resolved font handle for a text/choice field's appearance stream and /DA.
pub(crate) enum FieldFont<'a> {
    Standard {
        alias: &'a str,
        font_ref: ObjectId,
    },
    Embedded {
        alias: &'a str,
        type0_id: ObjectId,
        built: &'a BuiltFont,
        bytes: &'a [u8],
    },
}

/// Build one field's object graph (widget/field dict, /AP appearance, radio
/// kids) into `doc`. `font` is `Some` for text/choice fields, `None` otherwise.
/// Returns the top-level field id and the (page_index, widget_id) pairs the
/// caller must wire into page /Annots. Object add order matches the previous
/// inline construction so create output is byte-identical.
pub(crate) fn build_one_field(
    doc: &mut Document,
    field: &FieldDef,
    font: Option<FieldFont<'_>>,
) -> Result<BuiltField, String> {
    match field {
        // MOVE the existing `FieldDef::Text { .. } => { … }` arm body here.
        // Where it previously did `let (font_alias, font_ref) = font_registry[base_font];`
        // and `embedded_fonts[fid]` / `format!("BPF{fid}")`, instead read from `font`:
        //   Some(FieldFont::Embedded { alias, type0_id, built, bytes }) => embedded path,
        //   Some(FieldFont::Standard { alias, font_ref }) => standard path.
        // Instead of pushing to `acro_fields`/`page_annots`, return
        //   Ok(BuiltField { top_field_id: field_id, widgets: vec![(*page, field_id)] }).
        // ... Text arm ...
        // MOVE `FieldDef::CheckBox`, `FieldDef::Choice`, `FieldDef::Signature`
        // arm bodies similarly (they use `font` only for Choice; None for the
        // rest). Each returns BuiltField { top_field_id, widgets: vec![(*page, id)] }.
        // MOVE `FieldDef::RadioGroup`: parent field id = top_field_id; each kid
        // returns (kid.page, kid_widget_id) in `widgets`.
        _ => unreachable!("all FieldDef variants handled above"),
    }
}
```

Implementation note: this is a mechanical move of the four/five existing match arms out of `build_fields_and_acroform`'s `for field in fields` loop. The only edits are (a) reading font info from the `font: Option<FieldFont>` parameter instead of the local `font_registry`/`embedded_fonts`/`format!("BPF{fid}")`, and (b) returning `BuiltField` instead of pushing into `acro_fields`/`page_annots`.

- [ ] **Step 3: Rewrite `build_fields_and_acroform` to call `build_one_field`**

Keep the existing font-resolution prologue (the `needed` set, `dr_fonts`, `font_registry`, and the embedded-font `/BPF<n>` registration) unchanged. Replace the per-field construction loop body with:

```rust
    let mut acro_fields: Vec<Object> = Vec::new();
    let mut page_annots: Vec<Vec<ObjectId>> = vec![Vec::new(); page_ids.len()];

    for field in fields {
        let font = resolve_create_font(field, &font_registry, embedded_fonts, font_descs, fonts);
        let built = build_one_field(doc, field, font)?;
        acro_fields.push(Object::Reference(built.top_field_id));
        for (page_idx, widget_id) in built.widgets {
            page_annots[page_idx].push(widget_id);
        }
    }
```

Add a small local helper (private to create.rs) that reproduces today's per-field font lookup so aliases are unchanged:

```rust
fn resolve_create_font<'a>(
    field: &FieldDef,
    font_registry: &'a std::collections::HashMap<&str, (&'static str, ObjectId)>,
    embedded_fonts: &'a std::collections::HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &'a [FontDesc],
    fonts: &'a [u8],
) -> Option<FieldFont<'a>> {
    match field {
        FieldDef::Text { font_id: Some(i), .. } => {
            let (type0_id, built) = &embedded_fonts[i];
            // alias string must live long enough; store BPF<n> in a leaked/owned
            // form. Simplest: build_one_field takes &str, so pass a reference to
            // a String created here — instead, thread the alias through by
            // returning an owned wrapper. See note below.
            Some(FieldFont::Embedded { alias: /* "BPF{i}" */, type0_id: *type0_id, built, bytes: /* fonts[..] */ })
        }
        FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
            let base = font.as_deref().unwrap_or("Helvetica");
            let (alias, font_ref) = font_registry[base];
            Some(FieldFont::Standard { alias, font_ref })
        }
        _ => None,
    }
}
```

Alias-lifetime note: the embedded alias `BPF<n>` is a computed `String`. To keep `FieldFont` borrowing `&str`, precompute the embedded aliases into an owned `HashMap<usize, String>` in `build_fields_and_acroform` (built alongside the `/BPF<n>` `/DR` registration that already exists) and borrow from it in `resolve_create_font`. Pass that map in as an extra parameter. This keeps the canonical `Helv`/`BPF<n>` aliases, so create output is unchanged.

- [ ] **Step 4: Add a byte-identity regression test**

Append to the `tests` module in `crates/core/src/create.rs`:

```rust
    #[test]
    fn create_output_byte_identical_after_refactor() {
        // A doc exercising a standard-14 text field, a checkbox, and a choice
        // field. The bytes are pinned so the Task 1 refactor cannot change
        // create output. If create output legitimately changes, regenerate the
        // expected bytes deliberately.
        let ops = r#"[{"op":"addPage","width":300,"height":300}]"#;
        let fields = r#"[
            {"type":"text","name":"t","page":0,"x":10,"y":10,"width":100,"height":20},
            {"type":"checkBox","name":"c","page":0,"x":10,"y":40,"size":12},
            {"type":"choice","name":"d","page":0,"x":10,"y":70,"width":100,"height":20,"options":["a","b"],"combo":true}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        // Structural assertions (stable across environments):
        let doc = Document::load_mem(&out).unwrap();
        assert!(doc.catalog().unwrap().has(b"AcroForm"));
        let acro = get_first_field_dict(&doc); // existing test helper
        assert!(acro.has(b"T"));
    }
```

(If the repo already has a snapshot/golden mechanism for create output, prefer asserting equality against the pre-refactor bytes captured on a clean checkout. Otherwise the structural + existing create tests are the gate.)

- [ ] **Step 5: Run the create tests**

Run: `cargo test -p better-pdf-core create`
Expected: PASS — all existing create/field tests (e.g. `creates_text_field`, `creates_signature_field`, `text_field_align_and_font_size`, `da_font_alias_maps_all_standard_14`) still green, plus the new test.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/create.rs
git commit -m "refactor(create): extract build_one_field; parameterize field font alias

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `inject_fields` core — inject into a doc with no AcroForm

**Files:**
- Create: `crates/core/src/inject.rs`
- Modify: `crates/core/src/lib.rs` — add `mod inject;` and the `inject_fields` export.
- Modify: `crates/core/src/create.rs` — export the FontDesc parser or replicate the tiny `FontDesc` JSON parse (see Interfaces).
- Test: `crates/core/src/inject.rs` `tests` module.

**Interfaces:**
- Consumes (from Task 1): `create::FieldDef`, `create::build_one_field`, `create::BuiltField`, `create::FieldFont`, `create::da_font_alias`, `create::font_dict`.
- Consumes (existing): `draw::append_annot_to_page(inc, page_id, annot_id)` — make it `pub(crate)` (it is currently private in `draw.rs` ≈1878); `doc_io::load_pdf(data)`; `fonts::{build_embedded_font, EmbeddedFontInput, BuiltFont}`.
- Produces: `pub fn inject_fields_json(data: &[u8], fields_json: &str, fonts: &[u8], fonts_json: &str) -> Result<Vec<u8>, String>` and the wasm export `inject_fields`.

- [ ] **Step 1: Widen `append_annot_to_page` visibility**

In `crates/core/src/draw.rs`, change `fn append_annot_to_page(` (≈1878) to `pub(crate) fn append_annot_to_page(`.

- [ ] **Step 2: Write the failing test (inject a text field into a form-less doc)**

Create `crates/core/src/inject.rs` with a `tests` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    /// Build a 1-page PDF with NO form via the create path (empty fields).
    fn blank_page_pdf() -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            &[],
            "[]",
            "[]",
        )
        .unwrap()
    }

    #[test]
    fn injects_text_field_and_creates_acroform() {
        let base = blank_page_pdf();
        // Sanity: the base has no AcroForm.
        let base_doc = Document::load_mem(&base).unwrap();
        assert!(!base_doc.catalog().unwrap().has(b"AcroForm"));

        let fields =
            r#"[{"type":"text","name":"total","page":0,"x":10,"y":10,"width":100,"height":20,"value":"hi"}]"#;
        let out = inject_fields_json(&base, fields, &[], "[]").unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"AcroForm"), "AcroForm must be created");
        // /AcroForm/Fields has exactly our one field.
        let acro = match cat.get(b"AcroForm").unwrap() {
            lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            lopdf::Object::Dictionary(d) => d,
            _ => panic!("bad AcroForm"),
        };
        let fields_arr = acro.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields_arr.len(), 1);
        // The widget landed on page 0's /Annots.
        let pages = doc.get_pages();
        let (_, page0) = pages.into_iter().min_by_key(|(n, _)| *n).unwrap();
        let page = doc.get_dictionary(page0).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert!(!annots.is_empty(), "widget must be on page /Annots");
    }

    #[test]
    fn rejects_bad_page_index() {
        let base = blank_page_pdf();
        let fields = r#"[{"type":"text","name":"t","page":5,"x":1,"y":1,"width":10,"height":10}]"#;
        let err = inject_fields_json(&base, fields, &[], "[]").unwrap_err();
        assert!(err.contains("page"), "expected page-range error, got: {err}");
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p better-pdf-core inject`
Expected: FAIL — `inject_fields_json` not found (module not yet implemented).

- [ ] **Step 4: Implement `inject.rs` (no-AcroForm path)**

Write the module body above the `tests` module:

```rust
use crate::create::{
    BuiltField, FieldDef, FieldFont, build_one_field, da_font_alias, font_dict,
};
use crate::doc_io::load_pdf;
use crate::draw::append_annot_to_page;
use crate::fonts::{BuiltFont, EmbeddedFontInput, build_embedded_font};
use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, dictionary};
use std::collections::{BTreeSet, HashMap};

/// Font descriptor for the embedded-font blob (mirror of create.rs FontDesc).
#[derive(serde::Deserialize)]
struct FontDesc {
    offset: usize,
    length: usize,
    subset: bool,
}

pub fn inject_fields_json(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
) -> Result<Vec<u8>, String> {
    let fields: Vec<FieldDef> =
        serde_json::from_str(fields_json).map_err(|e| format!("invalid fields JSON: {e}"))?;
    if fields.is_empty() {
        return Ok(data.to_vec());
    }
    let font_descs: Vec<FontDesc> =
        serde_json::from_str(fonts_json).map_err(|e| format!("invalid fonts JSON: {e}"))?;

    let doc = load_pdf(data)?;

    // Collision check against existing top-level field names (see helper below).
    let existing_names = existing_field_names(&doc)?;
    for f in &fields {
        let name = field_name(f);
        if existing_names.contains(name) {
            return Err(format!("field name '{name}' already exists in this document"));
        }
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Resolve target page ids (0-based index into sorted pages), same as draw.rs.
    let page_ids: Vec<ObjectId> = {
        let prev = inc.get_prev_documents();
        let mut sorted: Vec<(u32, ObjectId)> = prev.get_pages().into_iter().collect();
        sorted.sort_by_key(|(n, _)| *n);
        sorted.into_iter().map(|(_, id)| id).collect()
    };
    for f in &fields {
        let pg = field_page(f);
        if pg >= page_ids.len() {
            return Err(format!("field page index {pg} out of range"));
        }
    }

    // Existing /DR/Font alias keys (to uniquify against). Empty when no AcroForm.
    let existing_aliases = existing_dr_aliases(&inc);

    // Build embedded fonts into new_document; map font_id -> (type0_id, BuiltFont).
    let embedded_fonts = build_embedded_for_fields(&mut inc, &fields, &font_descs, fonts)?;

    // Resolve standard-14 + embedded aliases, add /DR font objects to new_document.
    let mut dr_additions: Vec<(String, ObjectId)> = Vec::new();
    let std_aliases = resolve_std_aliases(&mut inc, &fields, &existing_aliases, &mut dr_additions);
    let emb_aliases = resolve_embedded_aliases(&fields, &embedded_fonts, &existing_aliases, &std_aliases, &mut dr_additions);

    // Build each field and wire widgets onto pages.
    let mut acro_field_ids: Vec<ObjectId> = Vec::new();
    for f in &fields {
        let font = field_font(f, &std_aliases, &emb_aliases, &embedded_fonts, &font_descs, fonts);
        let built: BuiltField = build_one_field(&mut inc.new_document, f, font)?;
        acro_field_ids.push(built.top_field_id);
        for (page_idx, widget_id) in built.widgets {
            append_annot_to_page(&mut inc, page_ids[page_idx], widget_id)?;
        }
    }

    // Attach a brand-new AcroForm (no-AcroForm path); Task 3 adds the merge path.
    attach_new_acroform(&mut inc, &acro_field_ids, &dr_additions)?;

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
```

Then implement the helpers in the same module:

```rust
fn field_name(f: &FieldDef) -> &str { /* match each variant, return &name */ }
fn field_page(f: &FieldDef) -> usize { /* match each variant, return *page (radio: first option page or its own) */ }

/// Top-level field /T names already in the document (empty if no AcroForm).
fn existing_field_names(doc: &lopdf::Document) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).map_err(|e| e.to_string())?;
    let cat = doc.get_dictionary(root).map_err(|e| e.to_string())?;
    let acro = match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    if let Some(acro) = acro {
        if let Ok(arr) = acro.get(b"Fields").and_then(Object::as_array) {
            for f in arr {
                if let Ok(id) = f.as_reference() {
                    if let Ok(fd) = doc.get_dictionary(id) {
                        if let Ok(t) = fd.get(b"T").and_then(Object::as_str) {
                            names.insert(String::from_utf8_lossy(t).into_owned());
                        }
                    }
                }
            }
        }
    }
    Ok(names)
}

/// Existing /DR/Font alias keys, so injected aliases can avoid collisions.
fn existing_dr_aliases(inc: &IncrementalDocument) -> BTreeSet<String> { /* walk prev catalog /AcroForm /DR /Font keys; empty if absent */ }

/// Build embedded fonts referenced by text fields into inc.new_document.
/// Mirrors create.rs ≈595–645 but adds objects via inc.new_document.add_object.
fn build_embedded_for_fields(
    inc: &mut IncrementalDocument,
    fields: &[FieldDef],
    font_descs: &[FontDesc],
    fonts: &[u8],
) -> Result<HashMap<usize, (ObjectId, BuiltFont)>, String> {
    let mut used_per_font: HashMap<usize, BTreeSet<char>> = HashMap::new();
    for f in fields {
        if let FieldDef::Text { font_id: Some(i), value, default_value, .. } = f {
            let set = used_per_font.entry(*i).or_default();
            if let Some(v) = value { set.extend(v.chars()); }
            if let Some(dv) = default_value { set.extend(dv.chars()); }
        }
    }
    let mut ids: Vec<usize> = used_per_font.keys().copied().collect();
    ids.sort_unstable();
    let mut out = HashMap::new();
    for id in ids {
        let fd = &font_descs[id];
        let bytes = &fonts[fd.offset..fd.offset + fd.length];
        let input = EmbeddedFontInput { data: bytes, subset: fd.subset, used_chars: used_per_font.remove(&id).unwrap_or_default() };
        let mut add = |o: Object| inc.new_document.add_object(o);
        let built = build_embedded_font(&mut add, &input)?;
        out.insert(id, built);
    }
    Ok(out)
}

/// Assign a unique alias for each standard-14 base font used, add its font dict
/// to new_document, and record (alias, id) in dr_additions. Returns base->alias/id.
fn resolve_std_aliases(
    inc: &mut IncrementalDocument,
    fields: &[FieldDef],
    existing: &BTreeSet<String>,
    dr_additions: &mut Vec<(String, ObjectId)>,
) -> HashMap<String, (String, ObjectId)> {
    let mut needed: BTreeSet<&str> = BTreeSet::new();
    needed.insert("Helvetica");
    for f in fields {
        if let FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } = f {
            needed.insert(font.as_deref().unwrap_or("Helvetica"));
        }
    }
    let mut used_aliases: BTreeSet<String> = existing.clone();
    let mut map = HashMap::new();
    for base in needed {
        let canonical = da_font_alias(base).expect("validated font"); // Task 1 pub(crate)
        let alias = uniquify(canonical, &mut used_aliases);
        let id = inc.new_document.add_object(Object::Dictionary(font_dict(base)));
        dr_additions.push((alias.clone(), id));
        map.insert(base.to_string(), (alias, id));
    }
    map
}

/// Assign unique /BPF-style aliases for embedded fonts and record them in /DR.
fn resolve_embedded_aliases(
    fields: &[FieldDef],
    embedded_fonts: &HashMap<usize, (ObjectId, BuiltFont)>,
    existing: &BTreeSet<String>,
    std_aliases: &HashMap<String, (String, ObjectId)>,
    dr_additions: &mut Vec<(String, ObjectId)>,
) -> HashMap<usize, String> {
    let mut used: BTreeSet<String> = existing.clone();
    for (a, _) in std_aliases.values() { used.insert(a.clone()); }
    let mut map = HashMap::new();
    let mut ids: Vec<usize> = embedded_fonts.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let alias = uniquify(&format!("BPF{id}"), &mut used);
        let (type0_id, _) = embedded_fonts[&id];
        dr_additions.push((alias.clone(), type0_id));
        map.insert(id, alias);
    }
    map
}

/// Return `base` if unused, else `base_1`, `base_2`, … Records the chosen alias.
fn uniquify(base: &str, used: &mut BTreeSet<String>) -> String {
    if !used.contains(base) { used.insert(base.to_string()); return base.to_string(); }
    let mut n = 1;
    loop {
        let cand = format!("{base}_{n}");
        if !used.contains(&cand) { used.insert(cand.clone()); return cand; }
        n += 1;
    }
}

/// Build the FieldFont for a field from the resolved alias maps.
fn field_font<'a>(
    f: &FieldDef,
    std_aliases: &'a HashMap<String, (String, ObjectId)>,
    emb_aliases: &'a HashMap<usize, String>,
    embedded_fonts: &'a HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &[FontDesc],
    fonts: &'a [u8],
) -> Option<FieldFont<'a>> {
    match f {
        FieldDef::Text { font_id: Some(i), .. } => {
            let (type0_id, built) = &embedded_fonts[i];
            let fd = &font_descs[*i];
            Some(FieldFont::Embedded { alias: &emb_aliases[i], type0_id: *type0_id, built, bytes: &fonts[fd.offset..fd.offset + fd.length] })
        }
        FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
            let base = font.as_deref().unwrap_or("Helvetica");
            let (alias, font_ref) = &std_aliases[base];
            Some(FieldFont::Standard { alias, font_ref: *font_ref })
        }
        _ => None,
    }
}

/// No-AcroForm path: create a fresh /AcroForm and attach it to the catalog.
fn attach_new_acroform(
    inc: &mut IncrementalDocument,
    field_ids: &[ObjectId],
    dr_additions: &[(String, ObjectId)],
) -> Result<(), String> {
    let mut dr_fonts = Dictionary::new();
    for (alias, id) in dr_additions { dr_fonts.set(alias.as_bytes().to_vec(), Object::Reference(*id)); }
    let acro = dictionary! {
        "Fields" => Object::Array(field_ids.iter().map(|id| Object::Reference(*id)).collect()),
        "DR" => Object::Dictionary(dictionary! { "Font" => Object::Dictionary(dr_fonts) }),
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
    };
    let acro_id = inc.new_document.add_object(Object::Dictionary(acro));
    // Attach to the catalog: clone the catalog into new_document and set /AcroForm.
    let root = inc.get_prev_documents().trailer.get(b"Root").and_then(Object::as_reference).map_err(|e| e.to_string())?;
    inc.opt_clone_object_to_new_document(root).map_err(|e| e.to_string())?;
    let cat = inc.new_document.get_object_mut(root).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
    cat.set("AcroForm", Object::Reference(acro_id));
    Ok(())
}
```

Implementation notes:
- `Object::as_str` / `as_reference` / `as_array` API names follow `lopdf` 0.41 as used in `fill.rs`/`draw.rs`; match those call sites if a name differs.
- `field_page` for a radio group: return the page of its first option (used only for the range check; per-kid pages are wired from `BuiltField.widgets`).

- [ ] **Step 5: Add the WASM export**

In `crates/core/src/lib.rs`, add `mod inject;` alongside the other `mod` lines, and:

```rust
/// Inject new AcroForm fields (JSON array of field defs, same schema as
/// create_document's fields_json) into a loaded PDF; returns new bytes.
/// `fonts` / `fonts_json` carry embedded fonts referenced by fields.
#[wasm_bindgen]
pub fn inject_fields(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
) -> Result<Vec<u8>, JsError> {
    inject::inject_fields_json(data, fields_json, fonts, fonts_json).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p better-pdf-core inject`
Expected: PASS — `injects_text_field_and_creates_acroform` and `rejects_bad_page_index` pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/inject.rs crates/core/src/lib.rs crates/core/src/draw.rs
git commit -m "feat(inject): inject_fields into loaded PDFs; create AcroForm when absent

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Merge into an existing AcroForm; collision + all-field-type coverage

**Files:**
- Modify: `crates/core/src/inject.rs` — replace `attach_new_acroform` call with a create-or-merge dispatcher; add merge helper.
- Test: `crates/core/src/inject.rs` `tests` module.

**Interfaces:**
- Consumes: everything from Task 2.
- Produces: `fn merge_or_create_acroform(inc, field_ids, dr_additions) -> Result<(), String>` replacing the direct `attach_new_acroform` call in `inject_fields_json`.

- [ ] **Step 1: Write the failing tests (existing form + collision + all types)**

Add to the `tests` module. `FICHA` is an existing AcroForm fixture (mirror the `const FICHA` include used in `create.rs`/`pageops.rs` tests):

```rust
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn top_field_names(bytes: &[u8]) -> std::collections::BTreeSet<String> {
        let doc = Document::load_mem(bytes).unwrap();
        // reuse existing_field_names via a thin wrapper, or re-walk here.
        crate::inject::test_field_names(&doc)
    }

    #[test]
    fn merges_into_existing_acroform_preserving_fields() {
        let before = top_field_names(FICHA);
        assert!(!before.is_empty(), "fixture must already have fields");
        let fields =
            r#"[{"type":"text","name":"bpf_new_field","page":0,"x":10,"y":10,"width":80,"height":18}]"#;
        let out = inject_fields_json(FICHA, fields, &[], "[]").unwrap();
        let after = top_field_names(&out);
        // Every pre-existing field survives, and our new one is present.
        for name in &before { assert!(after.contains(name), "lost field {name}"); }
        assert!(after.contains("bpf_new_field"));
    }

    #[test]
    fn rejects_name_collision_with_existing_field() {
        let existing = top_field_names(FICHA).into_iter().next().unwrap();
        let fields = format!(
            r#"[{{"type":"text","name":"{existing}","page":0,"x":1,"y":1,"width":10,"height":10}}]"#
        );
        let err = inject_fields_json(FICHA, &fields, &[], "[]").unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn injects_all_field_types() {
        let base = blank_page_pdf();
        let fields = r#"[
            {"type":"text","name":"txt","page":0,"x":10,"y":10,"width":100,"height":20},
            {"type":"text","name":"ml","page":0,"x":10,"y":40,"width":100,"height":40,"multiline":true},
            {"type":"checkBox","name":"cb","page":0,"x":10,"y":90,"size":12},
            {"type":"radioGroup","name":"rg","options":[{"value":"a","page":0,"x":10,"y":110,"size":12},{"value":"b","page":0,"x":40,"y":110,"size":12}]},
            {"type":"choice","name":"dd","page":0,"x":10,"y":140,"width":100,"height":20,"options":["x","y"],"combo":true},
            {"type":"choice","name":"lb","page":0,"x":10,"y":170,"width":100,"height":40,"options":["x","y"],"combo":false},
            {"type":"signature","name":"sig","page":0,"x":10,"y":220,"width":100,"height":40}
        ]"#;
        let out = inject_fields_json(&base, fields, &[], "[]").unwrap();
        let names = top_field_names(&out);
        for n in ["txt","ml","cb","rg","dd","lb","sig"] {
            assert!(names.contains(n), "missing field {n}");
        }
    }
```

Add a `#[cfg(test)] pub(crate) fn test_field_names(doc: &Document) -> BTreeSet<String>` wrapper in the module (delegates to `existing_field_names(doc).unwrap()`), so tests can reuse the walker.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p better-pdf-core inject`
Expected: `merges_into_existing_acroform_preserving_fields` FAILS (current code always creates a fresh AcroForm, dropping the existing one / detaching existing fields). Collision test may already pass (Task 2 added the check); all-types may pass for the no-form path.

- [ ] **Step 3: Implement create-or-merge**

Replace the `attach_new_acroform(&mut inc, &acro_field_ids, &dr_additions)?;` call in `inject_fields_json` with `merge_or_create_acroform(&mut inc, &acro_field_ids, &dr_additions)?;` and add:

```rust
/// Attach fields to an existing /AcroForm (append /Fields, merge /DR/Font) or
/// create one if absent. Uses the fill.rs clone-and-edit pattern for both the
/// indirect-reference and inline-in-catalog AcroForm storage forms.
fn merge_or_create_acroform(
    inc: &mut IncrementalDocument,
    field_ids: &[ObjectId],
    dr_additions: &[(String, ObjectId)],
) -> Result<(), String> {
    let root = inc.get_prev_documents().trailer.get(b"Root").and_then(Object::as_reference).map_err(|e| e.to_string())?;
    let acro_ref = match inc.get_prev_documents().get_dictionary(root).map_err(|e| e.to_string())?.get(b"AcroForm") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,      // inline in catalog
        _ => return attach_new_acroform(inc, field_ids, dr_additions), // absent
    };

    match acro_ref {
        Some(acro_id) => {
            inc.opt_clone_object_to_new_document(acro_id).map_err(|e| e.to_string())?;
            let acro = inc.new_document.get_object_mut(acro_id).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
            append_fields_and_dr(acro, field_ids, dr_additions);
        }
        None => {
            // Inline AcroForm lives in the catalog dict; clone the catalog.
            inc.opt_clone_object_to_new_document(root).map_err(|e| e.to_string())?;
            let cat = inc.new_document.get_object_mut(root).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
            let acro = cat.get_mut(b"AcroForm").and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
            append_fields_and_dr(acro, field_ids, dr_additions);
        }
    }
    Ok(())
}

/// Append new field refs to /Fields and new font aliases to /DR/Font, creating
/// either sub-dict if missing. Leaves /DA and /NeedAppearances untouched.
fn append_fields_and_dr(acro: &mut Dictionary, field_ids: &[ObjectId], dr_additions: &[(String, ObjectId)]) {
    // /Fields append
    match acro.get_mut(b"Fields") {
        Ok(Object::Array(arr)) => { for id in field_ids { arr.push(Object::Reference(*id)); } }
        _ => acro.set("Fields", Object::Array(field_ids.iter().map(|id| Object::Reference(*id)).collect())),
    }
    // /DR /Font merge (aliases already uniquified vs existing keys)
    let dr = match acro.get_mut(b"DR") {
        Ok(Object::Dictionary(d)) => d,
        _ => { acro.set("DR", Object::Dictionary(Dictionary::new())); acro.get_mut(b"DR").and_then(Object::as_dict_mut).unwrap() }
    };
    let font = match dr.get_mut(b"Font") {
        Ok(Object::Dictionary(d)) => d,
        _ => { dr.set("Font", Object::Dictionary(Dictionary::new())); dr.get_mut(b"Font").and_then(Object::as_dict_mut).unwrap() }
    };
    for (alias, id) in dr_additions { font.set(alias.as_bytes().to_vec(), Object::Reference(*id)); }
    // /DA: if absent, seed a default so viewers have a fallback.
    if acro.get(b"DA").is_err() { acro.set("DA", Object::string_literal("/Helv 0 Tf 0 g")); }
}
```

Note: when the existing AcroForm's `/DR/Font` is an indirect reference (not inline), clone that object too and edit it. Handle it the same way as `append_annot_to_page` handles an indirect `/Annots` (clone via `opt_clone_object_to_new_document`, then `get_object_mut`). Add that branch inside `append_fields_and_dr` or before calling it if `/DR` or `/Font` is a `Reference`.

`existing_dr_aliases` (Task 2) must read the same indirect-or-inline `/DR/Font`, so the uniquify set matches what merge writes into.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p better-pdf-core inject`
Expected: PASS — all five inject tests green.

- [ ] **Step 5: qpdf structural validation**

Add a test that writes the merged output and runs the repo's qpdf check helper (mirror how `pageops.rs`/`fill.rs` tests validate). If the repo validates via a shared test util, call it; otherwise assert `Document::load_mem(&out)` succeeds and `get_pages()` is non-empty (already covered) and rely on the CI qpdf gate.

Run: `cargo test -p better-pdf-core inject`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/inject.rs
git commit -m "feat(inject): merge into existing AcroForm; full field-type coverage

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: TypeScript wiring — `createForm()` on loaded docs + flush-on-getForm

**Files:**
- Modify: `src/core/wasm-bindings.ts` — add `inject_fields` to `RawBindings` and `injectFields` to `makeBindings`.
- Modify: `src/core/document.ts` — add `injectFields` to `CoreWasm`; drop the load-mode throw in `createForm()`; add `injectPendingFields()`; hook it into `getForm()` and `save()`.
- Modify: `src/core/wasm.ts` and `src/core/wasm-browser.ts` — add `injectFields` to the destructure lists.
- Test: `tests/forms/fields-on-loaded-docs.test.ts` (new).

**Interfaces:**
- Consumes (from Task 2/3): wasm export `inject_fields(data, fieldsJson, fonts, fontsJson) -> Uint8Array`.
- Produces: `CoreWasm.injectFields(data, fieldsJson, fonts?, fontsJson?)`; `PdfDocument.createForm()` works in load mode; `injectPendingFields()` private method.

- [ ] **Step 1: Rebuild WASM so the JS bindings expose the new export**

Run: `bun run build:wasm` (or the repo's documented wasm-pack build command; check `package.json` scripts / README).
Expected: `pkg-web/better_pdf_core.js` now exports `inject_fields`.

- [ ] **Step 2: Add the binding wrapper**

In `src/core/wasm-bindings.ts`, add to `RawBindings`:

```ts
  inject_fields(
    data: Uint8Array,
    fieldsJson: string,
    fonts: Uint8Array,
    fontsJson: string,
  ): Uint8Array;
```

and to the object returned by `makeBindings`:

```ts
    injectFields: (data, fieldsJson, fonts = EMPTY, fontsJson = "[]") =>
      (guard(), raw.inject_fields(data, fieldsJson, fonts, fontsJson)),
```

In `src/core/document.ts` `CoreWasm`, add:

```ts
  injectFields(
    data: Uint8Array,
    fieldsJson: string,
    fonts?: Uint8Array,
    fontsJson?: string,
  ): Uint8Array;
```

In `src/core/wasm.ts` and `src/core/wasm-browser.ts`, add `injectFields,` to the destructured export lists.

- [ ] **Step 3: Write the failing integration test**

Create `tests/forms/fields-on-loaded-docs.test.ts` (follow the style of existing form tests — check an existing test in `tests/` for the exact import paths and fixture-loading helper):

```ts
import { describe, expect, test } from "bun:test";
import { PdfDocument } from "../../src/index.js";
import { readFileSync } from "node:fs";

const FICHA = new Uint8Array(
  readFileSync(new URL("../fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf", import.meta.url)),
);

describe("fields on loaded docs", () => {
  test("createForm() on a loaded doc adds a fillable text field", async () => {
    const doc = await PdfDocument.load(FICHA);
    const form = doc.createForm();
    form.addTextField({ name: "bpf_added", page: 0, x: 40, y: 700, width: 120, height: 20 });

    // Flush + fill in the same session.
    doc.getForm().getTextField("bpf_added").setText("hello");
    const out = await doc.save();

    // Reload and confirm the value round-trips and pre-existing fields survive.
    const reopened = await PdfDocument.load(out);
    const rf = reopened.getForm();
    expect(rf.getFieldNames()).toContain("bpf_added");
    expect(rf.getTextField("bpf_added").value ?? "").toBe("hello");
  });

  test("createForm() after getForm() throws", async () => {
    const doc = await PdfDocument.load(FICHA);
    doc.createForm().addTextField({ name: "bpf_x", page: 0, x: 10, y: 10, width: 50, height: 15 });
    doc.getForm(); // builds the form
    expect(() => doc.createForm()).toThrow();
  });

  test("collision with an existing field throws at flush", async () => {
    const doc = await PdfDocument.load(FICHA);
    const existing = doc.getForm().getFieldNames()[0];
    // New doc instance so getForm() hasn't sealed createForm().
    const doc2 = await PdfDocument.load(FICHA);
    doc2.createForm().addTextField({ name: existing, page: 0, x: 10, y: 10, width: 50, height: 15 });
    expect(() => doc2.getForm()).toThrow(/already exists/);
  });

  test("loaded doc that never calls createForm() is unchanged", async () => {
    const a = await (await PdfDocument.load(FICHA)).save();
    const b = await (await PdfDocument.load(FICHA)).save();
    expect(a).toEqual(b); // no field-injection path touched
  });
});
```

(Adjust `getTextField(...).value` accessor name to the actual `PdfForm` API — check `src/forms/form.ts`; the read accessor may be `.getText()` or a `value` getter.)

- [ ] **Step 4: Run to verify failure**

Run: `bun test tests/forms/fields-on-loaded-docs.test.ts`
Expected: FAIL — `createForm()` throws in load mode (current behavior).

- [ ] **Step 5: Implement the TS flush**

In `src/core/document.ts`:

Replace the `createForm()` load-mode throw:

```ts
  createForm(): FormBuilder {
    if (this.form) {
      throw new PdfError(
        "createForm() must be called before getForm(); the form has already been built for this document",
      );
    }
    if (this.mode === "create") {
      this.assertNotSealed();
    }
    return new FormBuilder(this.fieldDefs, this.fieldNames);
  }
```

Add the flush method (mirrors `materializeCreatedForm`):

```ts
  /** Loaded-doc analogue of materializeCreatedForm: inject any pending
   *  builder fields into the bytes, then let the load-mode form path take over. */
  private injectPendingFields(): void {
    if (this.mode !== "load" || this.fieldDefs.length === 0) return;
    const { fonts, fontsJson } = this.drawQueue.toCreatePayload();
    let bytes: Uint8Array;
    try {
      bytes = this.wasm.injectFields(
        this.bytes,
        JSON.stringify(this.fieldDefs),
        fonts,
        fontsJson,
      );
    } catch (e) {
      throw toPdfError(e);
    }
    this.bytes = bytes;
    this.fieldDefs.length = 0;
    this.fieldNames.clear();
  }
```

Hook into `getForm()` — before constructing `PdfForm`:

```ts
  getForm(): PdfForm {
    if (this.mode === "create" && !this.sealed) {
      this.materializeCreatedForm();
    } else {
      this.injectPendingFields();
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
```

Hook into `save()` — after the create-mode short-circuit, before assembling the plan, so a loaded doc with pending fields but no `getForm()` call still flushes:

```ts
  async save(): Promise<Uint8Array> {
    if (this.mode === "create" && !this.sealed) {
      try {
        return this.buildCreatedBytes();
      } catch (e) {
        throw toPdfError(e);
      }
    }
    this.injectPendingFields(); // load-mode: bake any pending builder fields
    const form = this.form;
    // ... rest unchanged ...
```

Implementation note: `toCreatePayload()` returns `{ opsJson, images, fonts, fontsJson }`; only `fonts`/`fontsJson` are needed here (they carry every `embedFont`-registered font, matching what the create path passes). Ignore `opsJson`/`images`.

- [ ] **Step 6: Run the integration tests**

Run: `bun test tests/forms/fields-on-loaded-docs.test.ts`
Expected: PASS — all four tests.

- [ ] **Step 7: Run the full TS + Rust suites (no regressions)**

Run: `cargo test -p better-pdf-core && bun test`
Expected: PASS — existing created-doc `getForm()`, fill, flatten, and page-op tests all still green.

- [ ] **Step 8: Commit**

```bash
git add src/core/wasm-bindings.ts src/core/document.ts src/core/wasm.ts src/core/wasm-browser.ts tests/forms/fields-on-loaded-docs.test.ts
git commit -m "feat(forms): createForm() adds fields to loaded PDFs via inject_fields flush

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Docs, CHANGELOG, and version bump

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `README.md` (form-fields / limitations section)
- Modify: `package.json`, `crates/core/Cargo.toml` (version 1.8.1 → 1.9.0)
- Modify: `src/core/document.ts` — `createForm()` doc comment (drop "not supported on loaded PDFs").

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the `createForm()` doc comment**

In `src/core/document.ts`, update the `createForm()` JSDoc: remove the `@throws` line about load mode; state that on a loaded document the added fields are injected on the first `getForm()`/`save()`, and that all `createForm()` field-adds must precede the first `getForm()`.

- [ ] **Step 2: Update README**

Find the README section that states adding fields to loaded PDFs is unsupported (grep `README.md` for "loaded" / "createForm" / "not yet supported"). Replace with a short example:

```md
### Add fields to an existing PDF

const doc = await PdfDocument.load(bytes);
const form = doc.createForm();
form.addTextField({ name: "signature_date", page: 0, x: 72, y: 120, width: 160, height: 22 });
doc.getForm().getTextField("signature_date").setText("2026-07-02");
await doc.save();
```

Note the constraint: add all fields before the first `getForm()`. Note the current limitation: filling an embedded-font field created this way is not yet supported (throws at save).

- [ ] **Step 3: Update CHANGELOG**

Under `## [Unreleased]` → `### Added`, prepend:

```md
- **`createForm()` now works on documents opened with `PdfDocument.load()`.**
  Add new AcroForm fields (text, checkbox, radio, dropdown, list box, signature)
  to an existing PDF, then read/fill them via `getForm()` in the same session.
  Fields are injected on the first `getForm()`/`save()`; add all fields before
  calling `getForm()`. A field name that collides with an existing field is
  rejected. Filling an embedded-font field created this way is not yet supported.
```

- [ ] **Step 4: Bump the version**

Set `"version": "1.9.0"` in `package.json` and `version = "1.9.0"` in `crates/core/Cargo.toml`.

- [ ] **Step 5: Verify the build is clean**

Run: `cargo build -p better-pdf-core && bun test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add CHANGELOG.md README.md package.json crates/core/Cargo.toml src/core/document.ts
git commit -m "docs: fields on loaded PDFs; bump to 1.9.0

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-review notes (for the executor)

- **Byte-identity (Task 1)** is the highest-risk step: the create path must produce identical bytes. If the extraction changes object add order, existing create tests will catch it. Do not "improve" anything in the moved arm bodies.
- **`lopdf` 0.41 method names** (`as_str`, `as_reference`, `as_array`, `as_dict_mut`, `opt_clone_object_to_new_document`, `get_object_mut`, `IncrementalDocument::create_from`, `save_to`) are all used in `fill.rs`/`draw.rs` — copy the exact call shapes from there if a signature differs from this plan.
- **Indirect vs inline `/DR/Font` and `/AcroForm`**: both the collision-alias read (`existing_dr_aliases`) and the merge write must handle the indirect-reference case by cloning the referenced object into `new_document` before editing — same idiom as `append_annot_to_page` for indirect `/Annots`.
- **Embedded-font fill** remains guarded in `fill.rs` (out of scope); the integration test deliberately fills only a standard-14 field.
