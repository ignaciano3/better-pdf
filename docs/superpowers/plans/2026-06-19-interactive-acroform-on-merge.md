# Interactive AcroForm on Merge/Assemble Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When `PdfDocument.merge` / `assemble` / `copyPages` assemble pages that carry AcroForm widgets, rebuild a working `/AcroForm` in the output so those fields stay interactive (fillable) instead of being baked-in static appearances.

**Architecture:** All work is in the Rust core's page-assembler (`crates/core/src/pageops.rs`). After source docs are renumbered+merged and the new catalog/pages tree is built, a new step reconstructs `/AcroForm`: it collects the top-level field objects whose widgets sit on kept pages, deduplicates them, renames cross-source name collisions with a per-source prefix, merges each source's `/DR` font resources and `/DA`, sets `/NeedAppearances true` so viewers regenerate appearances, and attaches the rebuilt `/AcroForm` to the new catalog. Widget annotation objects already survive on their pages via `/Annots`; this step re-attaches the field tree above them.

**Tech Stack:** Rust (`lopdf` 0.41), `wasm-bindgen`; existing TS bindings (`PdfDocument.merge/assemble/copyPages`) and `getForm()`/`read_fields_json` for verification. Bun test runner + qpdf validation.

## Global Constraints

- Only `crates/core/src/pageops.rs` and test/doc files change. No new WASM entrypoints, no TS API surface change (existing `merge`/`assemble`/`copyPages`/`splitPages` signatures are untouched).
- `lopdf::Document` exposes public `objects` and `max_id`; `renumber_objects_with`, `get_pages`, `get_dictionary(_mut)`, `get_object`, `add_object`, `new_object_id`, `prune_objects`, `trailer` are all in use already in this file.
- **Object-id safety (critical, pre-existing invariant):** `merged.max_id` is set from the loop's final `next` BEFORE any `new_object_id`/`add_object` call (pageops.rs:119). Any new object the rebuild allocates must come AFTER that line. Do not move it.
- **Pruning order:** `merged.prune_objects()` (pageops.rs:165) drops anything unreachable from the new `/Catalog`. The rebuilt `/AcroForm` MUST be attached to the catalog BEFORE `prune_objects()` runs, or its fields/DR are pruned.
- **Name-collision policy:** a field name that appears in more than one source doc is renamed by prefixing its top-level partial name `/T` with `d{sourceIndex}_` (e.g. `d0_total`, `d1_total`). Non-colliding names are left unchanged.
- **Appearance policy:** set `/NeedAppearances true` on the rebuilt `/AcroForm`; merge each source `/DR` (union of `/Font` entries, first-writer-wins on a resource-name collision) and carry the first source's `/DA` if present.
- **XFA:** if a source AcroForm carries `/XFA`, do NOT copy the `/XFA` entry into the rebuilt AcroForm (the merged output is a plain AcroForm). Fields are still rebuilt from widgets.
- **No-form passthrough:** if no kept page carries a form widget, behavior is exactly as today (no `/AcroForm` added).
- Version bump for this feature: **0.15.0** (new backward-compatible behavior). Adjust if a different milestone ships first.

---

### Task 1: Capture per-source AcroForm data; rebuild minimal `/AcroForm` (Fields + NeedAppearances)

**Files:**
- Modify: `crates/core/src/pageops.rs` — `manipulate_pages_json` (the per-doc loop, lines 79-114; and after catalog build, before `prune_objects()` at line 165); add helper fns + a use of `std::collections::HashMap`.

**Interfaces:**
- Produces (private to pageops.rs):
  - `struct SourceForm { dr: Option<Object>, da: Option<Object>, top_fields: Vec<ObjectId> }`
  - `fn top_field_of(doc: &Document, annot: ObjectId) -> ObjectId` — walk `/Parent` from a widget to the highest ancestor (the top-level field); returns `annot` itself if it has no `/Parent`.
  - `fn rebuild_acroform(merged: &mut Document, catalog_id: ObjectId, kept_pages: &[ObjectId], sources: &[SourceForm])` — attaches a rebuilt `/AcroForm` to the catalog (no-op if no kept widget maps to a captured field).

**Context:** The per-doc loop renumbers each source's objects into merged-id space (line 105) and bulk-moves them (line 113). Capture each source's AcroForm data AFTER `renumber_objects_with` (so ids are already in merged space) and BEFORE `mem::take` empties `doc.objects`. The new catalog id is the value returned by `merged.add_object(...)` at line 156; `kept_pages` are the page ids pushed into `kids` (collect them while building `kids`).

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/core/src/pageops.rs`:

```rust
    #[test]
    fn merge_rebuilds_interactive_acroform() {
        // FICHA is an AcroForm PDF. Merging two copies must yield an output
        // whose catalog has an /AcroForm with a non-empty /Fields array.
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = cat.get(b"AcroForm").expect("merged output must have /AcroForm");
        let af = match af {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("AcroForm must be a dict or ref"),
        };
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        assert!(!fields.is_empty(), "/Fields must be non-empty");
        assert!(
            af.get(b"NeedAppearances")
                .ok()
                .and_then(|o| o.as_bool().ok())
                == Some(true),
            "NeedAppearances must be true"
        );
    }

    #[test]
    fn merge_without_form_adds_no_acroform() {
        // A non-form PDF stays form-less. Use a single extracted page from FICHA
        // is still a form; instead assert: assembling pages that carry no widgets
        // produces no /AcroForm. (FICHA pages do carry widgets, so this test
        // documents the no-op path via a synthetic check below.)
        // Build a 1-page doc from create() has no widgets — but pageops takes raw
        // bytes; reuse FICHA and assert the no-op branch via top_field_of fallback
        // is covered by the unit test in Step 1b instead.
    }
```

> Remove the empty `merge_without_form_adds_no_acroform` stub before committing if it has no body; the no-op path is exercised by the `top_field_of` unit test below. Keep only `merge_rebuilds_interactive_acroform` for this task.

- [ ] **Step 1b: Add a focused unit test for `top_field_of`**

Append to the `tests` module:

```rust
    #[test]
    fn top_field_of_walks_parent_to_root() {
        // Build a tiny doc: top field A (no Parent) -> kid widget W (Parent A).
        let mut d = Document::with_version("1.7");
        let a = d.new_object_id();
        let w = d.new_object_id();
        d.objects.insert(
            a,
            Object::Dictionary(dictionary! { "T" => Object::string_literal("A") }),
        );
        d.objects.insert(
            w,
            Object::Dictionary(dictionary! {
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "Parent" => Object::Reference(a),
            }),
        );
        assert_eq!(top_field_of(&d, w), a, "widget resolves to its top field");
        assert_eq!(top_field_of(&d, a), a, "a top field resolves to itself");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops 2>&1 | tail -20`
Expected: FAIL — `top_field_of` is undefined; `merge_rebuilds_interactive_acroform` fails with "merged output must have /AcroForm" (current code drops it).

- [ ] **Step 3a: Add imports + helpers**

At the top of `crates/core/src/pageops.rs`, extend the `use` line and imports:

```rust
use lopdf::{dictionary, Document, Object, ObjectId};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
```

Add these helpers above `manipulate_pages_json`:

```rust
/// AcroForm data captured from one source doc, in merged-id space.
struct SourceForm {
    dr: Option<Object>,
    da: Option<Object>,
    top_fields: Vec<ObjectId>,
}

/// Capture a source doc's AcroForm /DR, /DA, and top-level field ids.
/// Call AFTER `renumber_objects_with` so the returned ids/refs are in
/// merged-id space, and BEFORE the objects are moved out of `doc`.
fn capture_source_form(doc: &Document) -> SourceForm {
    let mut out = SourceForm { dr: None, da: None, top_fields: Vec::new() };
    let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return out;
    };
    let Ok(cat) = doc.get_dictionary(root) else { return out };
    let af = match cat.get(b"AcroForm") {
        Ok(Object::Reference(r)) => match doc.get_dictionary(*r) {
            Ok(d) => d,
            Err(_) => return out,
        },
        Ok(Object::Dictionary(d)) => d,
        _ => return out,
    };
    out.dr = af.get(b"DR").ok().cloned();
    out.da = af.get(b"DA").ok().cloned();
    if let Ok(fields) = af.get(b"Fields").and_then(|o| o.as_array()) {
        for f in fields {
            if let Ok(id) = f.as_reference() {
                out.top_fields.push(id);
            }
        }
    }
    out
}

/// Walk a widget annotation's /Parent chain to the top-level field id.
/// Returns `annot` if it has no /Parent (terminal field == widget).
fn top_field_of(doc: &Document, annot: ObjectId) -> ObjectId {
    let mut cur = annot;
    for _ in 0..128 {
        let Ok(d) = doc.get_dictionary(cur) else { break };
        match d.get(b"Parent").and_then(Object::as_reference) {
            Ok(p) => cur = p,
            Err(_) => break,
        }
    }
    cur
}
```

- [ ] **Step 3b: Capture per-source form data in the loop**

In `manipulate_pages_json`, declare a collector next to `per_doc_pages` (after line 77):

```rust
    let mut per_doc_pages: Vec<Vec<ObjectId>> = Vec::new();
    let mut sources: Vec<SourceForm> = Vec::new();
```

Then, inside the per-doc loop, AFTER `next = doc.max_id + 1;` (line 106) and BEFORE the bulk move at line 113, add:

```rust
        // Capture AcroForm data while ids are renumbered but objects still live
        // in `doc`. (Pushed in the same order as descs so source index aligns.)
        sources.push(capture_source_form(&doc));
```

- [ ] **Step 3c: Collect kept page ids while building kids**

In the plan loop (lines 124-145), declare a collector before the loop:

```rust
    let mut kids: Vec<Object> = Vec::with_capacity(plan.len());
    let mut used: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    let mut kept_pages: Vec<ObjectId> = Vec::with_capacity(plan.len());
```

Inside the loop, after `kids.push(Object::Reference(pid));` (line 144), add:

```rust
        kept_pages.push(pid);
```

- [ ] **Step 3d: Rebuild AcroForm before prune**

In `manipulate_pages_json`, after the catalog is created and `Root` set (after line 160) and BEFORE `merged.prune_objects();` (line 165), add:

```rust
    rebuild_acroform(&mut merged, catalog_id, &kept_pages, &sources);
```

Add the function (minimal version — Fields + NeedAppearances only; DR/DA/rename come in later tasks):

```rust
/// Reconstruct a working /AcroForm on the merged catalog from the field objects
/// whose widgets sit on kept pages. No-op when no kept widget maps to a field.
fn rebuild_acroform(
    merged: &mut Document,
    catalog_id: ObjectId,
    kept_pages: &[ObjectId],
    sources: &[SourceForm],
) {
    // Map each captured top-level field id to the source doc it came from.
    let mut field_src: HashMap<ObjectId, usize> = HashMap::new();
    for (si, s) in sources.iter().enumerate() {
        for &fid in &s.top_fields {
            field_src.entry(fid).or_insert(si);
        }
    }
    if field_src.is_empty() {
        return;
    }

    // Find top-level fields reachable from widgets on kept pages, in page order.
    let mut kept_fields: Vec<ObjectId> = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for &pid in kept_pages {
        let annot_ids: Vec<ObjectId> = match merged
            .get_dictionary(pid)
            .ok()
            .and_then(|pd| pd.get(b"Annots").ok())
            .and_then(|o| o.as_array().ok())
        {
            Some(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
            None => continue,
        };
        for aid in annot_ids {
            let top = top_field_of(merged, aid);
            if field_src.contains_key(&top) && seen.insert(top) {
                kept_fields.push(top);
            }
        }
    }
    if kept_fields.is_empty() {
        return;
    }

    let fields: Vec<Object> = kept_fields.iter().map(|&id| Object::Reference(id)).collect();
    let acroform_id = merged.add_object(dictionary! {
        "Fields" => Object::Array(fields),
        "NeedAppearances" => Object::Boolean(true),
    });
    if let Ok(cat) = merged.get_dictionary_mut(catalog_id) {
        cat.set("AcroForm", Object::Reference(acroform_id));
    }
}
```

> Note: `HashSet` is now imported at the top, so the inline `std::collections::HashSet` at line 123 can stay or be simplified to `HashSet` — leave it as-is to minimize diff, or simplify; either is fine.

- [ ] **Step 4: Run tests to verify they pass**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops 2>&1 | tail -20`
Expected: PASS — `merge_rebuilds_interactive_acroform`, `top_field_of_walks_parent_to_root`, and all pre-existing pageops tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/pageops.rs
git commit -m "feat(pageops): rebuild minimal interactive AcroForm on merge/assemble"
```

---

### Task 2: Merge `/DR` font resources and carry `/DA`

**Files:**
- Modify: `crates/core/src/pageops.rs` — `rebuild_acroform`.

**Interfaces:**
- Consumes: `SourceForm.dr`, `SourceForm.da` (Task 1); `field_src` source mapping (Task 1).
- Produces: the rebuilt `/AcroForm` now also carries a merged `/DR` (with a unioned `/Font` subdict) and a `/DA` string.

**Context:** `/DR` is a resource dictionary; its `/Font` subdict maps resource names (e.g. `/Helv`) to font object references. With `/NeedAppearances true` the viewer regenerates appearances using `/DR` fonts referenced by each field's `/DA`. Union the `/Font` entries across sources (first-writer-wins per resource name). Carry the first available `/DA`.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/core/src/pageops.rs`:

```rust
    #[test]
    fn merge_acroform_has_dr_fonts_and_da() {
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!(),
        };
        // /DR present with a /Font subdict that has at least one entry.
        let dr = af.get(b"DR").expect("AcroForm must carry /DR");
        let dr = match dr {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("DR must be dict/ref"),
        };
        let fonts = dr.get(b"Font").expect("DR must carry /Font");
        let fonts = match fonts {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("DR/Font must be dict/ref"),
        };
        assert!(!fonts.as_hashmap().is_empty(), "DR/Font must have entries");
        assert!(af.has(b"DA"), "AcroForm should carry a /DA");
    }
```

> If FICHA's AcroForm has no `/DA`, drop the final `af.has(b"DA")` assertion and assert only `/DR`/`/Font`. Verify FICHA's AcroForm contents first with a quick check; keep the assertions that match the fixture.

- [ ] **Step 2: Run test to verify it fails**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops::merge_acroform_has_dr_fonts_and_da 2>&1 | tail -20`
Expected: FAIL — "AcroForm must carry /DR" (Task 1's AcroForm has no `/DR`).

- [ ] **Step 3: Implement DR/DA merge**

In `rebuild_acroform`, replace the AcroForm construction block (the `let acroform_id = merged.add_object(dictionary! { ... });` and the catalog `set`) with:

```rust
    let fields: Vec<Object> = kept_fields.iter().map(|&id| Object::Reference(id)).collect();

    // Merge /DR /Font entries across sources (first-writer-wins per name).
    let mut merged_fonts = lopdf::Dictionary::new();
    let mut da: Option<Object> = None;
    for s in sources {
        if da.is_none() {
            if let Some(d) = &s.da {
                da = Some(d.clone());
            }
        }
        let Some(dr_obj) = &s.dr else { continue };
        let dr_dict = match dr_obj {
            Object::Reference(r) => merged.get_dictionary(*r).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        };
        let Some(dr_dict) = dr_dict else { continue };
        let font_obj = dr_dict.get(b"Font").ok().cloned();
        let font_dict = match font_obj {
            Some(Object::Reference(r)) => merged.get_dictionary(r).ok().cloned(),
            Some(Object::Dictionary(d)) => Some(d),
            _ => None,
        };
        if let Some(fd) = font_dict {
            for (k, v) in fd.iter() {
                if !merged_fonts.has(k) {
                    merged_fonts.set(k.to_vec(), v.clone());
                }
            }
        }
    }

    let mut dr = lopdf::Dictionary::new();
    if !merged_fonts.as_hashmap().is_empty() {
        dr.set("Font", Object::Dictionary(merged_fonts));
    }

    let mut acroform = dictionary! {
        "Fields" => Object::Array(fields),
        "NeedAppearances" => Object::Boolean(true),
    };
    if !dr.as_hashmap().is_empty() {
        acroform.set("DR", Object::Dictionary(dr));
    }
    if let Some(da) = da {
        acroform.set("DA", da);
    }
    let acroform_id = merged.add_object(acroform);
    if let Ok(cat) = merged.get_dictionary_mut(catalog_id) {
        cat.set("AcroForm", Object::Reference(acroform_id));
    }
```

> The `dictionary!` macro and `lopdf::Dictionary` are already available (the file uses `dictionary!` at line 150). `Dictionary::iter()` yields `(&Vec<u8>, &Object)`; `has`, `set`, `as_hashmap` are stable lopdf APIs.

- [ ] **Step 4: Run tests to verify they pass**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops 2>&1 | tail -20`
Expected: PASS — all pageops tests including the new DR/DA test.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/pageops.rs
git commit -m "feat(pageops): merge AcroForm /DR fonts and /DA into rebuilt form"
```

---

### Task 3: Rename cross-source field-name collisions

**Files:**
- Modify: `crates/core/src/pageops.rs` — `rebuild_acroform` (+ a small helper).

**Interfaces:**
- Consumes: `kept_fields`, `field_src` (Task 1).
- Produces: top-level fields whose partial name `/T` collides across source docs get `/T` rewritten to `d{sourceIndex}_{name}`.

**Context:** Merging two docs that share a field name (including `merge([FICHA, FICHA])`) yields duplicate fully-qualified names; PDF viewers would link them to one shared value. Per the chosen policy, rename only the colliding ones with a per-source prefix so each stays independently fillable.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module:

```rust
    #[test]
    fn merge_self_renames_colliding_field_names() {
        // Merging FICHA with itself: every field name appears in both sources,
        // so all kept top-level fields must be prefixed d0_/d1_ — yielding no
        // duplicate /T values among the rebuilt /Fields.
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!(),
        };
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        let mut names: Vec<String> = Vec::new();
        for f in fields {
            let fd = doc.get_dictionary(f.as_reference().unwrap()).unwrap();
            if let Ok(t) = fd.get(b"T").and_then(|o| o.as_str()) {
                names.push(String::from_utf8_lossy(t).into_owned());
            }
        }
        let unique: HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "no duplicate top-level /T names");
        assert!(
            names.iter().any(|n| n.starts_with("d0_")) && names.iter().any(|n| n.starts_with("d1_")),
            "colliding names must be per-source prefixed"
        );
    }
```

> `HashSet` is imported at module top (Task 1). If the test module needs it explicitly, add `use std::collections::HashSet;` inside `mod tests` or reference it via the top-level import.

- [ ] **Step 2: Run test to verify it fails**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops::merge_self_renames_colliding_field_names 2>&1 | tail -20`
Expected: FAIL — duplicate `/T` names (no renaming yet) → `unique.len() != names.len()`.

- [ ] **Step 3: Implement collision rename**

In `rebuild_acroform`, AFTER `kept_fields` is finalized (right after the `if kept_fields.is_empty() { return; }` guard) and BEFORE building `fields`, insert:

```rust
    // Detect partial-name collisions across SOURCE docs and rename them with a
    // per-source prefix so each field stays independently addressable.
    fn partial_name(doc: &Document, id: ObjectId) -> Option<String> {
        let d = doc.get_dictionary(id).ok()?;
        let t = d.get(b"T").ok()?.as_str().ok()?;
        Some(String::from_utf8_lossy(t).into_owned())
    }
    // name -> set of source indices that use it (among kept fields)
    let mut name_sources: HashMap<String, HashSet<usize>> = HashMap::new();
    for &fid in &kept_fields {
        if let (Some(name), Some(&si)) = (partial_name(merged, fid), field_src.get(&fid)) {
            name_sources.entry(name).or_default().insert(si);
        }
    }
    for &fid in &kept_fields {
        let (Some(name), Some(&si)) = (partial_name(merged, fid), field_src.get(&fid)) else {
            continue;
        };
        let collides = name_sources.get(&name).map(|s| s.len() > 1).unwrap_or(false);
        if collides {
            let new_name = format!("d{si}_{name}");
            if let Ok(d) = merged.get_dictionary_mut(fid) {
                d.set("T", Object::string_literal(new_name));
            }
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml pageops 2>&1 | tail -20`
Expected: PASS — all pageops tests including the rename test.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/pageops.rs
git commit -m "feat(pageops): rename cross-source AcroForm field-name collisions"
```

---

### Task 4: WASM rebuild, TS end-to-end verification, qpdf validation

**Files:**
- Test: `tests/page-operations.test.ts` (append form-preservation tests)
- Test: `tests/qpdf-validate.test.ts` (confirm merged-with-form output validates) — only if the existing harness makes adding a case easy; otherwise add a qpdf check inside the new page-operations test.
- No source change expected unless the e2e test surfaces a bug.

**Interfaces:**
- Consumes: `PdfDocument.merge` (`src/index.ts`), `doc.getForm().getFields()` / `read_fields_json` via `getForm()`.

**Context:** The Rust core is built to WASM by `bun run build:wasm`; tests run against the rebuilt artifact. After a merge of two form PDFs, loading the output and reading its fields must show interactive fields (a non-empty field list), and the names must be unique (renamed where they collided).

- [ ] **Step 1: Rebuild the WASM artifact**

Run: `source ~/.cargo/env && bun run build:wasm`
Expected: build succeeds; `pkg-web/` updated. (Required before the TS tests see the new Rust behavior.)

- [ ] **Step 2: Write the failing test**

Append to `tests/page-operations.test.ts` (reuse its existing `FIXTURE` / `PdfDocument` imports at the top of the file):

```ts
test("merge preserves interactive form fields", async () => {
  const src = readFileSync(FIXTURE);
  const merged = await PdfDocument.merge([src, src]);
  const doc = await PdfDocument.load(merged);
  const fields = doc.getForm().getFields();
  // Two copies merged => roughly double the original field count, all present.
  const single = (await PdfDocument.load(src)).getForm().getFields();
  expect(single.length).toBeGreaterThan(0);
  expect(fields.length).toBeGreaterThanOrEqual(single.length);
});

test("merged form field names are unique (collisions renamed)", async () => {
  const src = readFileSync(FIXTURE);
  const merged = await PdfDocument.merge([src, src]);
  const doc = await PdfDocument.load(merged);
  const names = doc.getForm().getFields().map((f) => f.name);
  const unique = new Set(names);
  expect(unique.size).toBe(names.length);
});
```

> Confirm `readFileSync` is imported in `tests/page-operations.test.ts`; add `import { readFileSync } from "node:fs";` if missing. Confirm `getForm().getFields()` returns objects with a `.name` (matches `FieldInfo.name`); adjust the accessor if the public field type differs.

- [ ] **Step 3: Run test to verify it fails (or passes if Rust already covers it)**

Run: `bun test tests/page-operations.test.ts`
Expected: Before the WASM rebuild it would fail; after Step 1's rebuild these should PASS given Tasks 1-3. If they fail, the failure pinpoints an e2e gap (e.g. `getForm` reads `/AcroForm/Fields` differently than the rebuild emits) — fix in `pageops.rs` and re-run.

- [ ] **Step 4: Validate output structure with qpdf**

Add a structural validation to the merge test (or a new case) that runs the existing qpdf path used by `tests/qpdf-validate.test.ts`. Mirror that file's invocation. Example shape (match the existing helper's real API):

```ts
test("merged-with-form output passes qpdf --check", async () => {
  const src = readFileSync(FIXTURE);
  const merged = await PdfDocument.merge([src, src]);
  // Reuse the project's qpdf validation helper / invocation from
  // tests/qpdf-validate.test.ts. Assert it reports no errors.
  await expectQpdfValid(merged); // <- replace with the actual helper used in the repo
});
```

> Read `tests/qpdf-validate.test.ts` first and copy its exact validation mechanism (helper name, how it spawns qpdf, how it asserts). Do not invent `expectQpdfValid` if the repo uses a different pattern.

- [ ] **Step 5: Run the full suite + cargo**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml 2>&1 | tail -5 && bun test 2>&1 | tail -5 && bunx tsc --noEmit`
Expected: cargo all pass; bun 0 fail; tsc clean.

- [ ] **Step 6: Commit**

```bash
git add tests/page-operations.test.ts tests/qpdf-validate.test.ts
git commit -m "test(pageops): verify interactive form survives merge end-to-end"
```

---

### Task 5: Docs, changelog, version bump

**Files:**
- Modify: `docs/site/src/content/docs/reference/limitations.md` (lines 31-43, the page-operations / non-interactive-AcroForm caveat)
- Modify: `docs/site/src/content/docs/migrating/from-pdf-lib.md` (any AcroForm-on-merge note)
- Modify: `skills/better-pdf/SKILL.md` (merge/assemble section)
- Modify: `CHANGELOG.md`
- Modify: `package.json`, `crates/core/Cargo.toml`, `crates/core/Cargo.lock`

**Interfaces:** none (docs/metadata only).

- [ ] **Step 1: Update limitations doc**

In `docs/site/src/content/docs/reference/limitations.md`, replace the "non-interactive AcroForm fields" caveat (lines 33-37) with the now-supported behavior:

```md
  - **Interactive form fields survive merge/assemble (0.15.0):** Pages merged
    or assembled from documents with AcroForm fields keep those fields
    **interactive** in the output — `/AcroForm` is rebuilt with the kept
    fields, merged `/DR` fonts, and `/NeedAppearances true`. Field names that
    collide across source documents are renamed with a per-source prefix
    (`d0_`, `d1_`, …) so each stays independently fillable.
    - **Caveat:** the same page selected twice (`assemble` with a duplicate
      `{docIndex, pageIndex}`) shares one field object, so its fields are
      linked rather than renamed. `/XFA` data is dropped (output is a plain
      AcroForm).
```

- [ ] **Step 2: Update migration + skill docs**

In `docs/site/src/content/docs/migrating/from-pdf-lib.md` and `skills/better-pdf/SKILL.md`, update any text that says merged forms lose interactivity to state that fields are preserved (0.15.0), with the collision-rename note.

- [ ] **Step 3: Update changelog**

In `CHANGELOG.md`, add under `## [Unreleased]`:

```md
## [0.15.0] - 2026-06-19

### Added

- `PdfDocument.merge` / `assemble` / `copyPages` now rebuild a working `/AcroForm` when assembled pages carry form widgets, so fields stay interactive (fillable) in the output. The rebuilt form merges each source's `/DR` fonts and `/DA` and sets `/NeedAppearances true`. Field names that collide across source documents are renamed with a per-source prefix (`d0_`, `d1_`, …).

### Notes

- A page selected more than once shares its field objects (linked, not renamed). `/XFA` data is not carried into the merged output.
```

- [ ] **Step 4: Bump versions**

- `package.json`: `"version": "0.15.0",`
- `crates/core/Cargo.toml`: `version = "0.15.0"`
- Refresh lock:

```bash
source ~/.cargo/env
cargo update -p better-pdf-core --precise 0.15.0 --manifest-path crates/core/Cargo.toml
```

- [ ] **Step 5: Verify green**

Run: `source ~/.cargo/env && cargo test --manifest-path crates/core/Cargo.toml 2>&1 | tail -5 && bun test 2>&1 | tail -5 && bunx tsc --noEmit`
Expected: all pass; tsc clean.

- [ ] **Step 6: Commit**

```bash
git add docs/site CHANGELOG.md package.json crates/core/Cargo.toml crates/core/Cargo.lock skills/better-pdf/SKILL.md
git commit -m "docs: document interactive AcroForm on merge; release 0.15.0"
```

---

## Final Whole-Branch Review

Dispatch the final code review (superpowers:requesting-code-review) on the most capable model — this branch mutates object graphs and the catalog, so verify: (1) object-id allocation stays collision-free (`merged.max_id` set before any `new_object_id`/`add_object`, including the rebuild's `add_object`); (2) the rebuilt `/AcroForm` is attached before `prune_objects()` so fields/DR survive; (3) `top_field_of` cannot loop on a cyclic `/Parent` (bounded to 128); (4) renamed `/T` values do not corrupt fields that have `/Kids` vs terminal field+widget; (5) the no-form passthrough path adds no `/AcroForm`. Then use superpowers:finishing-a-development-branch to merge to master (`--no-ff`). The user pushes manually.

## Self-Review Notes

- **Spec coverage:** rebuild `/AcroForm` with kept fields ✅ (T1); merged `/DR` + `/DA` + `/NeedAppearances` ✅ (T2); per-source collision rename ✅ (T3); e2e interactivity + qpdf ✅ (T4); docs/version ✅ (T5); no-form passthrough ✅ (T1 guard); XFA dropped ✅ (rebuild emits a fresh dict, never copies `/XFA`).
- **Type consistency:** `SourceForm { dr, da, top_fields }`, `top_field_of(doc, annot) -> ObjectId`, `rebuild_acroform(merged, catalog_id, kept_pages, sources)`, and `capture_source_form(doc) -> SourceForm` are referenced identically across tasks. `field_src: HashMap<ObjectId, usize>` and `kept_fields: Vec<ObjectId>` names are consistent T1→T3.
- **Object-id safety:** the rebuild's only allocation is `merged.add_object(acroform)`, which runs after `merged.max_id` is set (line 119) and after all source objects are moved — collision-free. It runs before `prune_objects()` so the AcroForm and its referenced fields/fonts are retained.
- **Ordering:** capture happens after `renumber_objects_with` (ids in merged space) and before `mem::take` (objects still present) — correct. `kept_pages` collected during `kids` build — correct.
- **Known v1 limitations (documented):** duplicate-page selection links fields; `/XFA` dropped; DR font union is first-writer-wins (acceptable under `NeedAppearances`).
