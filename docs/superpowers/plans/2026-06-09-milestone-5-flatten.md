# Milestone 5 — Flatten Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Permanently bake a field's appearance into the page (so the value becomes ordinary page graphics) and remove its widget + AcroForm entry — for one named field or all fields.

**Architecture:** A new Rust module `flatten.rs` exposes `flatten_fields_json(data, names_json)`. For each named field it: resolves the field's widgets (id, `/Rect`, page, and the appearance stream to stamp); for each widget, registers that existing `/AP/N` stream object as an XObject in its **page** `/Resources/XObject`, appends a `q cm /Name Do Q` draw to that page's `/Contents`, and removes the widget from the page `/Annots`; finally removes the field from the AcroForm `/Fields`. All via `IncrementalDocument` (append-only). The JS side queues flatten requests and `PdfDocument.save()` runs them **after** fills (fill generates `/AP`; flatten stamps it), as a second incremental pass.

**Tech Stack:** Rust (lopdf 0.41 `IncrementalDocument`, `Stream`, `add_object`), wasm-bindgen 0.2.123, bun test. No new crates.

---

## Verified facts (from de-risking probes, since removed)

Confirmed against `Form.-D.P.-2.4.1-Ficha-personal.pdf`:

- A widget carries `/P` (its page reference) — use it directly; fall back to scanning pages' `/Annots` only if absent.
- Page `/Contents` may be a single stream **Reference** (corpus) or an array; convert a single ref to `Array[old, drawRef]`, push onto an existing array.
- Page `/Resources` is a Reference to an object whose `/XObject` is an **inline dict**; clone that object and add `Name -> Reference(ap_stream_id)`.
- A flattened field's `/AP/N` **stream object** is referenced directly by id from the appended (later) incremental revision — cross-revision references resolve fine; no need to copy the stream.
- Removing a widget from page `/Annots` (array of refs) and the field from AcroForm `/Fields` (AcroForm is inline in the Catalog → clone Root, edit inline dict) works and reloads valid. Append-only confirmed (57,155 → 58,634 bytes for one field).
- Draw op for a generated appearance (BBox `[0 0 w h]` == rect size, no `/Matrix`): `q 1 0 0 1 x0 y0 cm /Name Do Q`. General case scales BBox→Rect.

---

## Scope decisions (and deferrals)

- **In:** flatten one field (`flattenField(name)`) or all (`flatten()`); text, choice, checkbox, radio (stamp the current `/AS` state's appearance); multi-widget fields; multi-page documents (resolve each widget's own page).
- **Stamp existing `/AP` only.** A field renders on flatten **iff it has an appearance** — which our fill (M4) always generates. Flattening a field that has a value but no `/AP` removes the widget without drawing the value. (Acceptable for v1: corpus forms start empty and are filled via our API. A future enhancement can auto-generate an appearance during flatten.)
- **Deferred:** appearance `/Matrix` rotation handling (corpus appearances have none — note as a limitation); removing now-unused AcroForm `/DR` resources; pruning empty `/AcroForm` after flatten-all (leaving `Fields []` is valid).
- **Graphics-state safety:** the appended draw stream wraps its ops in `q … Q` and sets its own CTM. Assumes existing page content is balanced (true for the corpus).

---

## File Structure

- **Create** `crates/core/src/flatten.rs` — the flatten engine + its tests.
- **Modify** `crates/core/src/fill.rs` — make `find_field` `pub(crate)` (reused to locate fields by name).
- **Modify** `crates/core/src/lib.rs` — `mod flatten;` + `#[wasm_bindgen] flatten_fields`.
- **Modify** `src/wasm.ts` — export `flattenFields`.
- **Modify** `src/form.ts` — `flatten()` / `flattenField(name)` queueing a `string[]`; expose it to `PdfDocument`.
- **Modify** `src/index.ts` — `save()` runs fills then flatten.
- **Create** `tests/flatten.test.ts` — public-API flatten → save → reload → assertions.
- **Modify** `examples/playground.ts` — demo flattening the filled field.

---

### Task 1: Rust flatten engine

**Files:** Create `crates/core/src/flatten.rs`; Modify `crates/core/src/fill.rs` (`pub(crate) fn find_field`), `crates/core/src/lib.rs` (`mod flatten;`).

- [ ] **Step 1: Expose `find_field`**

In `crates/core/src/fill.rs`, change `fn find_field` to `pub(crate) fn find_field`. In `crates/core/src/lib.rs`, add `mod flatten;` beside the other modules.

- [ ] **Step 2: Write the failing test**

Create `crates/core/src/flatten.rs`:

```rust
//! Flatten engine: bake a field's appearance into its page and remove the
//! widget + AcroForm entry. Operates on existing /AP streams (see plan).

pub fn flatten_fields_json(_data: &[u8], _names_json: &str) -> Result<Vec<u8>, String> {
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::flatten_fields_json;
    use crate::fill::fill_fields_json;
    use lopdf::{Document, Object};

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn field_names(bytes: &[u8]) -> Vec<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn flatten_removes_field_and_stamps_page() {
        // First fill so the text field has an /AP, then flatten it.
        let filled = fill_fields_json(
            FICHA,
            r#"[{"name":"beneficiario.apellidos_nombres","value":"FLAT"}]"#,
        ).unwrap();
        let out = flatten_fields_json(&filled, r#"["beneficiario.apellidos_nombres"]"#).unwrap();

        // Append-only over the filled bytes.
        assert!(out.len() > filled.len());
        assert_eq!(&out[..filled.len()], &filled[..]);

        // Field is gone from the AcroForm.
        let names = field_names(&out);
        assert!(!names.iter().any(|n| n == "beneficiario.apellidos_nombres"), "field still present: {names:?}");

        // Page /Contents references our XObject and is now an array; still valid PDF.
        let doc = Document::load_mem(&out).unwrap();
        let (_, &pid) = doc.get_pages().iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        assert!(matches!(page.get(b"Contents"), Ok(Object::Array(_))));
    }

    #[test]
    fn flatten_unknown_field_errors() {
        let err = flatten_fields_json(FICHA, r#"["nope.nope"]"#).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }
}
```

- [ ] **Step 3: Run to confirm failure**

Run: `cargo test --manifest-path crates/core/Cargo.toml flatten::tests`
Expected: FAIL (`not implemented`).

- [ ] **Step 4: Implement the engine**

Replace the stub:

```rust
use crate::fill::find_field;
use crate::forms;
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use serde::Deserialize;

/// One widget to flatten: where it is and what appearance to stamp.
struct WidgetStamp {
    widget_id: ObjectId,
    page_id: ObjectId,
    rect: [f32; 4],
    /// Appearance stream id + its BBox, or None when the widget has no drawable AP.
    ap: Option<(ObjectId, [f32; 4])>,
}

pub fn flatten_fields_json(data: &[u8], names_json: &str) -> Result<Vec<u8>, String> {
    let names: Vec<String> = serde_json::from_str(names_json).map_err(|e| e.to_string())?;
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;

    // Resolve everything against the immutable doc first.
    let mut field_ids: Vec<ObjectId> = Vec::new();
    let mut stamps: Vec<WidgetStamp> = Vec::new();
    for name in &names {
        let (field_id, dict) = find_field(&doc, name)
            .ok_or_else(|| format!("no such field: {name}"))?;
        field_ids.push(field_id);
        for w in field_widgets(&doc, field_id, dict) {
            stamps.push(resolve_stamp(&doc, w)?);
        }
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    let mut counter = 0usize;
    for s in &stamps {
        stamp_widget(&mut inc, s, &mut counter)?;
        remove_annot(&mut inc, s.page_id, s.widget_id)?;
    }
    remove_fields(&mut inc, &field_ids)?;

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// (widget_id, page_id, rect) for each of a field's widgets. A field with no
/// /Kids is its own widget.
struct RawWidget {
    id: ObjectId,
    page_id: ObjectId,
    rect: [f32; 4],
}

fn field_widgets(doc: &Document, field_id: ObjectId, dict: &Dictionary) -> Vec<RawWidget> {
    let ids: Vec<ObjectId> = dict
        .get(b"Kids").and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let ids = if ids.is_empty() { vec![field_id] } else { ids };
    ids.into_iter().filter_map(|id| {
        let d = doc.get_dictionary(id).ok()?;
        let rect = read_rect(d)?;
        let page_id = d.get(b"P").ok().and_then(|o| o.as_reference().ok())
            .or_else(|| find_page_of_annot(doc, id))?;
        Some(RawWidget { id, page_id, rect })
    }).collect()
}

fn read_rect(d: &Dictionary) -> Option<[f32; 4]> {
    let a = d.get(b"Rect").ok()?.as_array().ok()?;
    let mut r = [0f32; 4];
    for (i, v) in a.iter().enumerate().take(4) { r[i] = v.as_float().unwrap_or(0.0); }
    Some(r)
}

/// Find the page whose /Annots contains `annot` (fallback when /P is absent).
fn find_page_of_annot(doc: &Document, annot: ObjectId) -> Option<ObjectId> {
    for (_, &pid) in doc.get_pages().iter() {
        if let Ok(page) = doc.get_dictionary(pid) {
            if let Ok(annots) = page.get(b"Annots").and_then(|o| o.as_array()) {
                if annots.iter().any(|o| o.as_reference().ok() == Some(annot)) {
                    return Some(pid);
                }
            }
        }
    }
    None
}

/// Resolve the appearance stream to stamp for a widget (selecting the /AS state
/// for button appearance subdictionaries).
fn resolve_stamp(doc: &Document, w: RawWidget) -> Result<WidgetStamp, String> {
    let d = doc.get_dictionary(w.id).map_err(|e| e.to_string())?;
    let ap = appearance_stream_id(doc, d).and_then(|id| {
        let bbox = doc.get_object(id).ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| read_rect(&s.dict).or(Some([0.0, 0.0, w.rect[2] - w.rect[0], w.rect[3] - w.rect[1]])))?;
        Some((id, bbox))
    });
    Ok(WidgetStamp { widget_id: w.id, page_id: w.page_id, rect: w.rect, ap })
}

/// The id of the appearance stream a widget currently shows.
fn appearance_stream_id(doc: &Document, widget: &Dictionary) -> Option<ObjectId> {
    let ap = forms::as_dict(doc, widget.get(b"AP").ok()?).ok()?;
    match ap.get(b"N").ok()? {
        Object::Reference(id) => Some(*id), // text/choice: N is the stream
        Object::Dictionary(states) => {
            // button: pick the /AS state's stream
            let as_name = widget.get(b"AS").ok()?.as_name().ok()?;
            states.get(as_name).ok()?.as_reference().ok()
        }
        _ => None,
    }
}

/// Stamp one widget's appearance onto its page.
fn stamp_widget(inc: &mut IncrementalDocument, s: &WidgetStamp, counter: &mut usize) -> Result<(), String> {
    let Some((ap_id, bbox)) = s.ap else { return Ok(()) }; // nothing to draw
    let name = format!("bpdfAp{counter}");
    *counter += 1;

    // 1) register the appearance stream as an XObject in the page resources.
    let res_id = page_resources_id(inc, s.page_id)?;
    inc.opt_clone_object_to_new_document(res_id).map_err(|e| e.to_string())?;
    {
        let res = dict_mut(inc, res_id)?;
        if !res.has(b"XObject") {
            res.set("XObject", Object::Dictionary(Dictionary::new()));
        }
        let xobj = res.get_mut(b"XObject").and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
        xobj.set(name.as_bytes().to_vec(), Object::Reference(ap_id));
    }

    // 2) append a draw stream to the page contents (BBox -> Rect transform).
    let (bw, bh) = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
    let (sx, sy) = (
        if bw != 0.0 { (s.rect[2] - s.rect[0]) / bw } else { 1.0 },
        if bh != 0.0 { (s.rect[3] - s.rect[1]) / bh } else { 1.0 },
    );
    let tx = s.rect[0] - bbox[0] * sx;
    let ty = s.rect[1] - bbox[1] * sy;
    let draw = format!("q {sx:.4} 0 0 {sy:.4} {tx:.2} {ty:.2} cm /{name} Do Q");
    let draw_id = inc.new_document.add_object(Object::Stream(
        Stream::new(Dictionary::new(), draw.into_bytes()).with_compression(false),
    ));

    inc.opt_clone_object_to_new_document(s.page_id).map_err(|e| e.to_string())?;
    let page = dict_mut(inc, s.page_id)?;
    let contents = page.get(b"Contents").map_err(|e| e.to_string())?.clone();
    let arr = match contents {
        Object::Array(mut a) => { a.push(Object::Reference(draw_id)); a }
        single => vec![single, Object::Reference(draw_id)],
    };
    page.set("Contents", Object::Array(arr));
    Ok(())
}

/// Remove a widget reference from a page's /Annots.
fn remove_annot(inc: &mut IncrementalDocument, page_id: ObjectId, widget: ObjectId) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(page_id).map_err(|e| e.to_string())?;
    let page = dict_mut(inc, page_id)?;
    if let Ok(annots) = page.get(b"Annots").and_then(|o| o.as_array()) {
        let kept: Vec<Object> = annots.iter()
            .filter(|o| o.as_reference().ok() != Some(widget))
            .cloned().collect();
        page.set("Annots", Object::Array(kept));
    }
    Ok(())
}

/// Remove fields from the AcroForm /Fields (AcroForm inline in Catalog, or a ref).
fn remove_fields(inc: &mut IncrementalDocument, field_ids: &[ObjectId]) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let root = prev.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    let acro_is_ref = matches!(cat.get(b"AcroForm"), Ok(Object::Reference(_)));
    if acro_is_ref {
        let id = cat.get(b"AcroForm").unwrap().as_reference().unwrap();
        inc.opt_clone_object_to_new_document(id).map_err(|e| e.to_string())?;
        filter_fields(dict_mut(inc, id)?, field_ids);
    } else {
        inc.opt_clone_object_to_new_document(root).map_err(|e| e.to_string())?;
        let cat = dict_mut(inc, root)?;
        let acro = cat.get_mut(b"AcroForm").and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
        filter_fields(acro, field_ids);
    }
    Ok(())
}

fn filter_fields(acro: &mut Dictionary, field_ids: &[ObjectId]) {
    if let Ok(fields) = acro.get(b"Fields").and_then(|o| o.as_array()) {
        let kept: Vec<Object> = fields.iter()
            .filter(|o| o.as_reference().ok().map(|id| !field_ids.contains(&id)).unwrap_or(true))
            .cloned().collect();
        acro.set("Fields", Object::Array(kept));
    }
}

/// The id of the object holding a page's /Resources (assumes a reference, as in
/// the corpus; inline resources are cloned via the page itself).
fn page_resources_id(inc: &mut IncrementalDocument, page_id: ObjectId) -> Result<ObjectId, String> {
    let prev = inc.get_prev_documents();
    let page = prev.get_dictionary(page_id).map_err(|e| e.to_string())?;
    match page.get(b"Resources") {
        Ok(Object::Reference(id)) => Ok(*id),
        _ => Err("page /Resources is not a reference (inline resources unsupported in v1)".into()),
    }
}

fn dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.new_document.get_object_mut(id).and_then(Object::as_dict_mut).map_err(|e| e.to_string())
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Unused; // (placeholder to keep serde import used if needed)
```

> NOTE TO IMPLEMENTER: drop the `Unused`/`Deserialize` placeholder — `serde_json::from_str::<Vec<String>>` does not need a derive. Keep only the imports actually used; run clippy to confirm. If `page /Resources` is inline rather than a reference in some fixture, the v1 error message above is acceptable (corpus uses a reference); do not over-engineer.

- [ ] **Step 5: Run to confirm pass + clippy**

Run: `cargo test --manifest-path crates/core/Cargo.toml flatten::tests`
Then: `cargo clippy --manifest-path crates/core/Cargo.toml --all-targets -- -D warnings`
Expected: both flatten tests pass; clippy clean. Fix any unused-import/lint inline.

- [ ] **Step 6: Full Rust suite**

Run: `cargo test --manifest-path crates/core/Cargo.toml`
Expected: all prior tests (21) + the 2 new flatten tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/flatten.rs crates/core/src/fill.rs crates/core/src/lib.rs
git commit -m "feat(core): flatten fields — stamp appearance, remove widget + field entry"
```

---

### Task 2: Expose `flatten_fields` across the WASM boundary

**Files:** Modify `crates/core/src/lib.rs`, `src/wasm.ts`.

- [ ] **Step 1: wasm-bindgen export**

In `crates/core/src/lib.rs` add:

```rust
/// Flatten the named fields (JSON array of names) and return new PDF bytes.
#[wasm_bindgen]
pub fn flatten_fields(data: &[u8], names_json: &str) -> Result<Vec<u8>, JsError> {
    flatten::flatten_fields_json(data, names_json).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 2: Rebuild wasm**

Run: `bun run build:wasm`
Expected: succeeds; `pkg/better_pdf_core.js` exports `flatten_fields`.

- [ ] **Step 3: Surface in `src/wasm.ts`**

```ts
export function flattenFields(data: Uint8Array, namesJson: string): Uint8Array {
  return core.flatten_fields(data, namesJson);
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/lib.rs src/wasm.ts
git commit -m "feat: expose flatten_fields across the wasm boundary"
```

---

### Task 3: TS API — `flatten()` / `flattenField()` + save sequencing + tests

**Files:** Modify `src/form.ts`, `src/index.ts`; Create `tests/flatten.test.ts`.

- [ ] **Step 1: Write the failing integration test**

Create `tests/flatten.test.ts`:

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const load = () => PdfDocument.load(new Uint8Array(readFileSync(FICHA)));

test("fill then flatten removes the field but keeps the document", async () => {
  const doc = await load();
  const form = doc.getForm();
  form.getTextField("beneficiario.apellidos_nombres").setText("FLAT");
  form.flattenField("beneficiario.apellidos_nombres");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  const names = reloaded.getForm().getFields().map((f) => f.name);
  expect(names).not.toContain("beneficiario.apellidos_nombres");
});

test("flatten() removes all fields", async () => {
  const doc = await load();
  doc.getForm().flatten();
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getForm().getFields().length).toBe(0);
});

test("flattenField on a missing field throws", async () => {
  const form = (await load()).getForm();
  expect(() => form.flattenField("nope.nope")).toThrow(/no such field/);
});
```

- [ ] **Step 2: Run to confirm failure**

Run: `bun test tests/flatten.test.ts`
Expected: FAIL (`flattenField` not a function).

- [ ] **Step 3: Add flatten queueing to `src/form.ts`**

Add a flatten list and methods to `PdfForm`:

```ts
  /** @internal — fully-qualified names queued for flattening. */
  readonly flattenQueue: string[] = [];

  /** Queue a single field to be flattened on save. */
  flattenField(name: string): void {
    if (!this.getField(name)) throw new Error(`no such field: ${name}`);
    if (!this.flattenQueue.includes(name)) this.flattenQueue.push(name);
  }

  /** Queue all fields to be flattened on save. */
  flatten(): void {
    for (const f of this.fields) {
      if (!this.flattenQueue.includes(f.name)) this.flattenQueue.push(f.name);
    }
  }
```

- [ ] **Step 4: Sequence fill → flatten in `src/index.ts` `save()`**

```ts
  async save(): Promise<Uint8Array> {
    const form = this.form;
    let bytes = this.bytes;
    if (form && form.queue.length > 0) {
      bytes = fillFields(bytes, form.queue.toJSON());
    }
    if (form && form.flattenQueue.length > 0) {
      bytes = flattenFields(bytes, JSON.stringify(form.flattenQueue));
    }
    if (bytes === this.bytes) {
      return roundTrip(this.bytes);
    }
    return bytes;
  }
```

Add `flattenFields` to the import from `./wasm.ts`.

- [ ] **Step 5: Run tests + type-check**

Run: `bun test` then `bunx tsc --noEmit`
Expected: all TS tests pass (12 prior + 3 new flatten), no type errors.

- [ ] **Step 6: Commit**

```bash
git add src/form.ts src/index.ts tests/flatten.test.ts
git commit -m "feat: PdfForm.flatten()/flattenField() applied after fills on save"
```

---

### Task 4: Playground demo

**Files:** Modify `examples/playground.ts`.

- [ ] **Step 1: Demo flatten**

After the existing fill demo block in `examples/playground.ts`, add:

```ts
  // --- Milestone 5 demo: flatten that field so it becomes page graphics. ---
  doc.getForm().flattenField(firstText.name);
  const flat = await doc.save();
  const flatPath = join(import.meta.dir, `flat-${basename(inputPath)}`);
  writeFileSync(flatPath, flat);
  const stillThere = (await PdfDocument.load(flat)).getForm().getField(firstText.name);
  console.log(`Flattened '${firstText.name}' → field present after flatten: ${stillThere ? "yes" : "no"}`);
  console.log(`Wrote:    ${flatPath} (${flat.length.toLocaleString()} bytes)`);
```

Add `examples/flat-*.pdf` to `.gitignore`.

- [ ] **Step 2: Run it**

Run: `bun run play`
Expected: prints `field present after flatten: no` and writes a `flat-*.pdf`.

- [ ] **Step 3: Commit**

```bash
git add examples/playground.ts .gitignore
git commit -m "chore: playground demonstrates flattening a field"
```

---

## Self-Review notes (for the controller)

- **Spec coverage (§2 flatten one / all):** `flattenField(name)` ✅, `flatten()` ✅; stamps text/choice/checkbox/radio current appearance; removes widgets + field entries. Consumes the M4 `/AP` streams.
- **Composition:** `save()` applies fills first (generates `/AP`), then flatten (stamps it) — two incremental passes layered append-only.
- **Deferred (noted):** `/Matrix` rotation, inline page `/Resources` (errors clearly), DR cleanup, AcroForm pruning. None occur in the corpus.
- **Type consistency:** `WidgetStamp{widget_id,page_id,rect,ap}`, `RawWidget{id,page_id,rect}` used consistently. `find_field` reused from `fill` (now `pub(crate)`).
- **No-op safety:** flattening a widget with no `/AP` removes it without drawing (documented behavior).
