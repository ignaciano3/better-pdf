# Milestone 3 — Fill Values + Incremental Save Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let callers set text/checkbox/radio/dropdown field values through the public TS API and persist them via an append-only incremental save, so a re-parse reads the new values back.

**Architecture:** Mutations are accumulated as plain `{name, value}` ops in JS (coarse boundary). `PdfDocument.save()` hands the original bytes + ops JSON to one Rust call, `fill_fields`. Rust re-derives each field's type from the PDF, mutates `/V` (and widget `/AS` for buttons, `/I` for dropdowns) on a `lopdf::IncrementalDocument`, and appends a new revision. Appearance-stream generation is **out of scope** (Milestone 4); the corpus already sets `/NeedAppearances true`, so viewers regenerate appearances and re-parse asserts on `/V`.

**Tech Stack:** Rust (lopdf 0.41 `IncrementalDocument`, serde), wasm-bindgen 0.2.123, TypeScript, bun test.

---

## Verified API facts (from de-risking probe, already removed)

These were confirmed empirically against `tests/fixtures/.../Form.-D.P.-2.4.1-Ficha-personal.pdf`:

- `IncrementalDocument::create_from(prev_bytes: Vec<u8>, prev_doc: Document)` builds an incremental doc whose `new_document` you mutate.
- `inc.opt_clone_object_to_new_document(id: ObjectId)` copies an existing object into the new revision so it can be edited (idempotent — safe to call twice for the same id).
- `inc.new_document.get_object_mut(id)?.as_dict_mut()?.set("V", obj)` mutates it.
- `inc.save_to(&mut Vec<u8>)` writes `prev_bytes` then appends only the changed objects + a new xref (append-only: 57,155 → 57,622 bytes for one text field).
- After save, `Document::load_mem(&out)` re-parses and `/V` reads back correctly for both a text field (`Object::String`) and a radio group (`Object::Name`).
- `Object::string_literal(s)`, `Object::Name(Vec<u8>)`, `Object::Integer(i64)`, `Object::Array(Vec<Object>)` are the constructors used.
- Field entries in `/AcroForm/Fields` and widget `/Kids` are `Object::Reference(ObjectId)` in this corpus, so they are addressable by id (`obj.as_reference()`).

---

## File Structure

- **Create** `crates/core/src/fill.rs` — fill engine: parse ops → resolve fields → mutate → incremental save. Keeps `forms.rs` focused on read.
- **Modify** `crates/core/src/forms.rs` — expose the small shared helpers `as_dict`, `name_part`, `parent_of`, `classify`, `MAX_PARENT_DEPTH`, `collect_on_states`, `opt_export` to `fill.rs` (make them `pub(crate)`), so locating fields and reading on-states is not duplicated.
- **Modify** `crates/core/src/lib.rs` — add `mod fill;` and `#[wasm_bindgen] fill_fields`.
- **Modify** `src/wasm.ts` — export `fillFields(data, opsJson)`.
- **Create** `src/fields.ts` — typed field wrappers (`PdfTextField`, `PdfCheckBox`, `PdfRadioGroup`, `PdfDropdown`) + the `FillOp` type + a `FillQueue` they push to.
- **Modify** `src/form.ts` — add typed accessors (`getTextField`, `getCheckBox`, `getRadioGroup`, `getDropdown`), hold the shared `FillQueue`, expose `pendingOps()`.
- **Modify** `src/index.ts` — cache the `PdfForm`; `save()` applies pending ops via `fillFields` (else `roundTrip`).
- **Create** `tests/fill.test.ts` — public-API fill → save → reload → assert, plus error cases.
- **Modify** `crates/core/src/fill.rs` `#[cfg(test)]` — Rust unit tests for each field kind + error paths.
- **Modify** `examples/playground.ts` — demonstrate a fill round-trip.

---

## Op data model (shared contract)

JSON crossing the boundary is an array of:

```json
[{ "name": "beneficiario.apellidos_nombres", "value": "HELLO" }]
```

- `name` — fully-qualified field name (same string `read_fields` returns).
- `value` — for text/dropdown: the literal string; for checkbox/radio: the on-state export value, or the literal `"Off"` to clear.

Rust re-derives the field **type** from the PDF (never trusts JS for it) and applies the right mutation. JS validates type/value up front for good error messages; Rust validates again and errors on unknown field name or invalid on-state/option.

---

### Task 1: Rust fill engine — resolve + mutate + incremental save

**Files:**
- Create: `crates/core/src/fill.rs`
- Modify: `crates/core/src/forms.rs` (widen helper visibility)
- Test: `crates/core/src/fill.rs` (`#[cfg(test)]` module)

- [ ] **Step 1: Widen shared helpers in `forms.rs`**

Change the visibility of these items from private to `pub(crate)` (only the keyword changes; bodies stay identical): `MAX_PARENT_DEPTH`, `fn as_dict`, `fn name_part`, `fn parent_of`, `fn classify`, `fn collect_on_states`, `fn opt_export`, `fn fully_qualified_name`, `fn inherited_name`, `fn inherited_int`. For example:

```rust
pub(crate) const MAX_PARENT_DEPTH: usize = 128;
pub(crate) fn as_dict<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Dictionary, String> { /* unchanged */ }
pub(crate) fn name_part(d: &Dictionary) -> Option<String> { /* unchanged */ }
pub(crate) fn parent_of<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a Dictionary> { /* unchanged */ }
pub(crate) fn classify(ft: &str, ff: i64) -> &'static str { /* unchanged */ }
pub(crate) fn collect_on_states(doc: &Document, widget: &Dictionary, out: &mut Vec<String>) { /* unchanged */ }
pub(crate) fn opt_export(o: &Object) -> String { /* unchanged */ }
pub(crate) fn fully_qualified_name(doc: &Document, d: &Dictionary) -> String { /* unchanged */ }
pub(crate) fn inherited_name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> { /* unchanged */ }
pub(crate) fn inherited_int(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<i64> { /* unchanged */ }
```

Run `cargo build --manifest-path crates/core/Cargo.toml` — expect a clean build (no behavior change).

- [ ] **Step 2: Write the failing test for text fill**

Add to the bottom of `crates/core/src/fill.rs` (create the file with just the test module + a stub first):

```rust
//! Fill engine: apply {name,value} ops to a PDF and incrementally save.

pub fn fill_fields_json(_data: &[u8], _ops_json: &str) -> Result<Vec<u8>, String> {
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::fill_fields_json;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn reparse_value(bytes: &[u8], field_name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array().unwrap().iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["value"].as_str().map(|s| s.to_string()))
    }

    #[test]
    fn fills_text_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA, IGNACIO"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        // Append-only: output starts with the original bytes.
        assert!(out.len() > FICHA.len());
        assert_eq!(&out[..FICHA.len()], FICHA);
        // Re-parse via the public reader.
        assert_eq!(reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(), Some("GARCIA, IGNACIO"));
        // And it is still a loadable PDF.
        Document::load_mem(&out).unwrap();
    }
}
```

- [ ] **Step 3: Run it to confirm failure**

Run: `cargo test --manifest-path crates/core/Cargo.toml fill::tests::fills_text_field`
Expected: FAIL (`not implemented`).

- [ ] **Step 4: Implement the fill engine**

Replace the stub in `crates/core/src/fill.rs` with the full implementation (keep the test module):

```rust
//! Fill engine: apply {name,value} ops to a PDF and incrementally save.

use crate::forms::{self};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId};
use serde::Deserialize;

#[derive(Deserialize)]
struct FillOp {
    name: String,
    value: String,
}

/// Apply the given fill ops to `data` and return new PDF bytes (incremental save).
pub fn fill_fields_json(data: &[u8], ops_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<FillOp> = serde_json::from_str(ops_json).map_err(|e| e.to_string())?;
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;

    // Resolve every op against the immutable doc first, so we can move `doc`
    // into the IncrementalDocument afterwards.
    let mut plan: Vec<Resolved> = Vec::with_capacity(ops.len());
    for op in &ops {
        plan.push(resolve(&doc, op)?);
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    for r in &plan {
        apply(&mut inc, r)?;
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// What to do to one field, pre-computed from the immutable document.
struct Resolved {
    field_id: ObjectId,
    apply: Apply,
}

enum Apply {
    /// Set /V to a string literal.
    Text(String),
    /// Set /V to a string literal and, if matched, /I to [index].
    Dropdown { value: String, index: Option<i64> },
    /// Set group /V to a Name, and each widget's /AS (on-state name or "Off").
    Button { value: String, widgets: Vec<(ObjectId, bool)> },
}

/// Locate the field for `op.name`, classify it, and build the mutation plan.
fn resolve(doc: &Document, op: &FillOp) -> Result<Resolved, String> {
    let (field_id, dict) = find_field(doc, &op.name)
        .ok_or_else(|| format!("no such field: {}", op.name))?;
    let ft = forms::inherited_name(doc, dict, b"FT").unwrap_or_default();
    let ff = forms::inherited_int(doc, dict, b"Ff").unwrap_or(0);
    let kind = forms::classify(&ft, ff);

    let apply = match kind {
        "text" => Apply::Text(op.value.clone()),
        "checkbox" | "radio" => {
            let widgets = button_widgets(doc, field_id, dict, &op.value)?;
            Apply::Button { value: op.value.clone(), widgets }
        }
        "dropdown" | "listbox" => {
            let index = dropdown_index(dict, &op.value);
            if op.value != "Off" && index.is_none() && has_opt(dict) {
                return Err(format!("'{}' is not a valid option for {}", op.value, op.name));
            }
            Apply::Dropdown { value: op.value.clone(), index }
        }
        other => return Err(format!("cannot fill field {} of type {}", op.name, other)),
    };
    Ok(Resolved { field_id, apply })
}

/// Resolve the button's widget set and validate the requested on-state.
/// Returns (widget_id, has_target_state) for each widget. A field with no
/// /Kids is its own widget.
fn button_widgets(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    value: &str,
) -> Result<Vec<(ObjectId, bool)>, String> {
    let mut widgets: Vec<(ObjectId, bool)> = Vec::new();
    let kid_ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let targets: Vec<ObjectId> = if kid_ids.is_empty() { vec![field_id] } else { kid_ids };

    let mut any_match = false;
    for id in targets {
        let has = doc
            .get_dictionary(id)
            .ok()
            .map(|w| widget_has_state(doc, w, value))
            .unwrap_or(false);
        if has {
            any_match = true;
        }
        widgets.push((id, has));
    }
    if value != "Off" && !any_match {
        return Err(format!("'{}' is not a valid on-state for this button", value));
    }
    Ok(widgets)
}

/// True if a widget's /AP/N has a sub-key named `state`.
fn widget_has_state(doc: &Document, widget: &Dictionary, state: &str) -> bool {
    let mut found = Vec::new();
    forms::collect_on_states(doc, widget, &mut found);
    found.iter().any(|s| s == state)
}

fn has_opt(dict: &Dictionary) -> bool {
    dict.get(b"Opt").and_then(|o| o.as_array()).map(|a| !a.is_empty()).unwrap_or(false)
}

/// Index of `value` within /Opt (matching export value), if present.
fn dropdown_index(dict: &Dictionary, value: &str) -> Option<i64> {
    let arr = dict.get(b"Opt").ok()?.as_array().ok()?;
    arr.iter().position(|o| forms::opt_export(o) == value).map(|i| i as i64)
}

/// Walk /AcroForm/Fields (and /Kids) to find the field whose fully-qualified
/// name equals `name`. Only reference-addressable fields are considered.
fn find_field<'a>(doc: &'a Document, name: &str) -> Option<(ObjectId, &'a Dictionary)> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    let acro = forms::as_dict(doc, catalog.get(b"AcroForm").ok()?).ok()?;
    let entries = acro.get(b"Fields").ok()?.as_array().ok()?;
    let mut stack: Vec<ObjectId> = entries.iter().filter_map(|e| e.as_reference().ok()).collect();
    let mut seen = 0usize;
    while let Some(id) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break; // guard against pathological/cyclic field trees
        }
        let Ok(d) = doc.get_dictionary(id) else { continue };
        if forms::fully_qualified_name(doc, d) == name {
            return Some((id, d));
        }
        if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
            for k in kids {
                if let Ok(kid_id) = k.as_reference() {
                    stack.push(kid_id);
                }
            }
        }
    }
    None
}

/// Apply one resolved mutation onto the incremental document.
fn apply(inc: &mut IncrementalDocument, r: &Resolved) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(r.field_id).map_err(|e| e.to_string())?;
    match &r.apply {
        Apply::Text(value) => {
            field_dict_mut(inc, r.field_id)?.set("V", Object::string_literal(value.as_str()));
        }
        Apply::Dropdown { value, index } => {
            let d = field_dict_mut(inc, r.field_id)?;
            d.set("V", Object::string_literal(value.as_str()));
            match index {
                Some(i) => { d.set("I", Object::Array(vec![Object::Integer(*i)])); }
                None => { d.remove(b"I"); }
            }
        }
        Apply::Button { value, widgets } => {
            field_dict_mut(inc, r.field_id)?
                .set("V", Object::Name(value.as_bytes().to_vec()));
            for (wid, has) in widgets {
                inc.opt_clone_object_to_new_document(*wid).map_err(|e| e.to_string())?;
                let as_state = if value != "Off" && *has { value.as_str() } else { "Off" };
                field_dict_mut(inc, *wid)?
                    .set("AS", Object::Name(as_state.as_bytes().to_vec()));
            }
        }
    }
    Ok(())
}

fn field_dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.new_document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Run the text test to confirm it passes**

Run: `cargo test --manifest-path crates/core/Cargo.toml fill::tests::fills_text_field`
Expected: PASS.

- [ ] **Step 6: Add the remaining Rust tests (radio, checkbox, dropdown, errors)**

Append to the `#[cfg(test)] mod tests` in `crates/core/src/fill.rs`:

```rust
    fn reparse_field<'a>(bytes: &'a [u8]) -> serde_json::Value {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn fills_radio_group() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        assert_eq!(reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(), Some("Titular"));
    }

    #[test]
    fn fills_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","value":"Casado"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        assert_eq!(reparse_value(&out, "beneficiario.estado_civil").as_deref(), Some("Casado"));
    }

    #[test]
    fn rejects_unknown_field() {
        let ops = r#"[{"name":"does.not.exist","value":"x"}]"#;
        let err = fill_fields_json(FICHA, ops).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_radio_state() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Nope"}]"#;
        let err = fill_fields_json(FICHA, ops).unwrap_err();
        assert!(err.contains("on-state"), "got: {err}");
    }

    #[test]
    fn applies_multiple_ops_in_one_save() {
        let ops = r#"[
            {"name":"beneficiario.apellidos_nombres","value":"A"},
            {"name":"beneficiario.tipo_beneficiario","value":"Familiar"}
        ]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        let f = reparse_field(&out);
        let by = |n: &str| f.as_array().unwrap().iter().find(|x| x["name"] == n).cloned().unwrap();
        assert_eq!(by("beneficiario.apellidos_nombres")["value"], "A");
        assert_eq!(by("beneficiario.tipo_beneficiario")["value"], "Familiar");
    }
```

> NOTE TO IMPLEMENTER: The fixture's exact field names and the dropdown's valid options were verified during Milestone 2 (`beneficiario.tipo_beneficiario` → states `Titular`/`Familiar`; `beneficiario.estado_civil` options include `Soltero`/`Casado`/`Divorciado`/`Viudo`). If a name/option assertion fails, run `cargo test ... -- --nocapture` and inspect `read_fields_json` output for the real strings rather than guessing.

- [ ] **Step 7: Run the full Rust suite**

Run: `cargo test --manifest-path crates/core/Cargo.toml`
Expected: all tests pass (Milestone 2 tests + the 6 new fill tests). Also run `cargo clippy --manifest-path crates/core/Cargo.toml -- -D warnings` and fix any lint.

- [ ] **Step 8: Commit**

```bash
git add crates/core/src/fill.rs crates/core/src/forms.rs
git commit -m "feat(core): incremental fill for text/checkbox/radio/dropdown"
```

---

### Task 2: Expose `fill_fields` across the WASM boundary

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `src/wasm.ts`

- [ ] **Step 1: Add the wasm-bindgen export**

In `crates/core/src/lib.rs`, add `mod fill;` next to `mod forms;`, then add:

```rust
/// Apply fill ops (JSON array of {name, value}) to a PDF and return new bytes.
#[wasm_bindgen]
pub fn fill_fields(data: &[u8], ops_json: &str) -> Result<Vec<u8>, JsError> {
    fill::fill_fields_json(data, ops_json).map_err(|e| JsError::new(&e))
}
```

- [ ] **Step 2: Rebuild the wasm package**

Run: `bun run build:wasm`
Expected: succeeds; `pkg/better_pdf_core.js` now exports `fill_fields`.

- [ ] **Step 3: Surface it in `src/wasm.ts`**

Add below `readFields`:

```ts
export function fillFields(data: Uint8Array, opsJson: string): Uint8Array {
  return core.fill_fields(data, opsJson);
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/lib.rs src/wasm.ts
git commit -m "feat: expose fill_fields across the wasm boundary"
```

---

### Task 3: Typed field wrappers + fill queue in the TS API

**Files:**
- Create: `src/fields.ts`
- Modify: `src/form.ts`

- [ ] **Step 1: Create `src/fields.ts`**

```ts
import type { FieldInfo } from "./form.ts";

/** One queued mutation: set field `name` to `value`. */
export interface FillOp {
  name: string;
  value: string;
}

/** Shared, ordered list of pending mutations for a document. */
export class FillQueue {
  private readonly ops: FillOp[] = [];
  push(op: FillOp): void {
    this.ops.push(op);
  }
  toJSON(): string {
    return JSON.stringify(this.ops);
  }
  get length(): number {
    return this.ops.length;
  }
}

/** A text field. */
export class PdfTextField {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Set the field's text value. */
  setText(value: string): void {
    this.queue.push({ name: this.info.name, value });
  }
}

/** A checkbox. */
export class PdfCheckBox {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Check the box using its real on-state export value. */
  check(): void {
    const on = this.info.states[0];
    if (!on) throw new Error(`checkbox '${this.info.name}' has no on-state`);
    this.queue.push({ name: this.info.name, value: on });
  }
  /** Uncheck the box. */
  uncheck(): void {
    this.queue.push({ name: this.info.name, value: "Off" });
  }
}

/** A radio-button group. */
export class PdfRadioGroup {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid export values for this group. */
  get options(): string[] {
    return this.info.states;
  }
  /** Select an option by its real export value. */
  select(value: string): void {
    if (!this.info.states.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for radio '${this.info.name}' (valid: ${this.info.states.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}

/** A dropdown (choice) field. */
export class PdfDropdown {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid option export values. */
  get options(): string[] {
    return this.info.options;
  }
  /** Select an option by its real export value. */
  select(value: string): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for dropdown '${this.info.name}' (valid: ${this.info.options.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}
```

- [ ] **Step 2: Add typed accessors + queue to `src/form.ts`**

Change `src/form.ts` to hold a `FillQueue` and add the accessors. The class becomes:

```ts
import { readFields } from "./wasm.ts";
import {
  FillQueue,
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
} from "./fields.ts";

export type FieldType =
  | "text" | "checkbox" | "radio" | "dropdown"
  | "listbox" | "signature" | "pushbutton" | "unknown";

export interface FieldInfo {
  /** Fully-qualified field name (ancestor /T joined by "."). */
  name: string;
  type: FieldType;
  /** Current value as a string, or null if unset. */
  value: string | null;
  /** On-state export values for checkbox/radio; empty otherwise. */
  states: string[];
  /** Option export values for dropdown/listbox; empty otherwise. */
  options: string[];
  readOnly: boolean;
}

/** A view over a PDF's AcroForm fields, with typed mutation accessors. */
export class PdfForm {
  private readonly fields: FieldInfo[];
  /** @internal — shared with PdfDocument so save() can flush pending ops. */
  readonly queue = new FillQueue();

  /** @internal */
  constructor(bytes: Uint8Array) {
    this.fields = JSON.parse(readFields(bytes)) as FieldInfo[];
  }

  getFields(): FieldInfo[] {
    return this.fields;
  }

  getField(name: string): FieldInfo | undefined {
    return this.fields.find((f) => f.name === name);
  }

  private require(name: string, type: FieldType): FieldInfo {
    const f = this.getField(name);
    if (!f) throw new Error(`no such field: ${name}`);
    if (f.type !== type) {
      throw new Error(`field '${name}' is a ${f.type}, not a ${type}`);
    }
    return f;
  }

  getTextField(name: string): PdfTextField {
    return new PdfTextField(this.require(name, "text"), this.queue);
  }
  getCheckBox(name: string): PdfCheckBox {
    return new PdfCheckBox(this.require(name, "checkbox"), this.queue);
  }
  getRadioGroup(name: string): PdfRadioGroup {
    return new PdfRadioGroup(this.require(name, "radio"), this.queue);
  }
  getDropdown(name: string): PdfDropdown {
    return new PdfDropdown(this.require(name, "dropdown"), this.queue);
  }
}
```

- [ ] **Step 3: Type-check**

Run: `bunx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/fields.ts src/form.ts
git commit -m "feat: typed field wrappers and fill queue"
```

---

### Task 4: `PdfDocument.save()` flushes pending fills + integration tests

**Files:**
- Modify: `src/index.ts`
- Test: `tests/fill.test.ts` (create)

- [ ] **Step 1: Write the failing integration test**

Create `tests/fill.test.ts`:

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

function load() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
}

test("fills a text field and reads it back after save", async () => {
  const doc = await load();
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});

test("selects a radio option and reads it back", async () => {
  const doc = await load();
  doc.getForm().getRadioGroup("beneficiario.tipo_beneficiario").select("Titular");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.tipo_beneficiario")?.value).toBe("Titular");
});

test("selects a dropdown option and reads it back", async () => {
  const doc = await load();
  doc.getForm().getDropdown("beneficiario.estado_civil").select("Casado");
  const out = await doc.save();

  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.estado_civil")?.value).toBe("Casado");
});

test("save with no pending ops returns a byte-identical round-trip", async () => {
  const original = new Uint8Array(readFileSync(FICHA));
  const doc = await PdfDocument.load(original);
  const out = await doc.save();
  expect(Buffer.from(out).equals(Buffer.from(original))).toBe(true);
});

test("wrong-type access throws", async () => {
  const form = (await load()).getForm();
  expect(() => form.getRadioGroup("beneficiario.apellidos_nombres")).toThrow(/not a radio/);
});

test("invalid radio option throws before save", async () => {
  const form = (await load()).getForm();
  expect(() => form.getRadioGroup("beneficiario.tipo_beneficiario").select("Nope")).toThrow();
});
```

- [ ] **Step 2: Run it to confirm failure**

Run: `bun test tests/fill.test.ts`
Expected: FAIL (fills not applied; `getForm()` returns a fresh form each call, so the queue is lost).

- [ ] **Step 3: Update `src/index.ts` to cache the form and flush ops**

```ts
import { roundTrip, fillFields } from "./wasm.ts";
import { PdfForm } from "./form.ts";

/**
 * A loaded PDF document. Holds the source bytes, exposes the AcroForm, and
 * persists queued field mutations on `save()` via an incremental update.
 */
export class PdfDocument {
  private form?: PdfForm;

  /** @internal */
  private constructor(private readonly bytes: Uint8Array) {}

  static async load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument> {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    return new PdfDocument(bytes);
  }

  /** Serialize back to PDF bytes, applying any queued fills (incremental). */
  async save(): Promise<Uint8Array> {
    const form = this.form;
    if (form && form.queue.length > 0) {
      return fillFields(this.bytes, form.queue.toJSON());
    }
    return roundTrip(this.bytes);
  }

  /** The document's AcroForm. The same instance is returned each call, so
   *  queued mutations accumulate until `save()`. */
  getForm(): PdfForm {
    if (!this.form) this.form = new PdfForm(this.bytes);
    return this.form;
  }
}

export { PdfForm } from "./form.ts";
export type { FieldInfo, FieldType } from "./form.ts";
export {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
} from "./fields.ts";
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `bun test tests/fill.test.ts`
Expected: all 6 pass.

- [ ] **Step 5: Run the whole suite + type-check**

Run: `bun test` then `bunx tsc --noEmit`
Expected: all TS tests pass (roundtrip + fields + fill), no type errors.

- [ ] **Step 6: Commit**

```bash
git add src/index.ts tests/fill.test.ts
git commit -m "feat: PdfDocument.save() applies queued fills incrementally"
```

---

### Task 5: Playground demo + docs polish

**Files:**
- Modify: `examples/playground.ts`

- [ ] **Step 1: Extend the playground to fill the first text field**

After the existing field-listing block in `examples/playground.ts`, add:

```ts
// --- Milestone 3 demo: fill the first writable text field and re-read it. ---
const firstText = fields.find((f) => f.type === "text" && !f.readOnly);
if (firstText) {
  doc.getForm().getTextField(firstText.name).setText("better-pdf was here");
  const filled = await doc.save();
  const filledPath = join(import.meta.dir, `filled-${basename(inputPath)}`);
  writeFileSync(filledPath, filled);
  const check = (await PdfDocument.load(filled)).getForm().getField(firstText.name);
  console.log(`\nFilled '${firstText.name}' → "${check?.value}"`);
  console.log(`Wrote:    ${filledPath} (${filled.length.toLocaleString()} bytes)`);
}
```

> NOTE: `doc.save()` above uses the cached form, so the queued `setText` is applied. The earlier round-trip `save()` in the file runs before any op is queued, so it stays byte-identical — leave it as is.

- [ ] **Step 2: Add `filled-*` outputs to `.gitignore`**

Add a line `examples/filled-*.pdf` to `.gitignore` (next to the existing `examples/out-*.pdf`).

- [ ] **Step 3: Run the playground**

Run: `bun run play`
Expected: prints `Filled '<name>' → "better-pdf was here"` and writes a `filled-*.pdf`.

- [ ] **Step 4: Commit**

```bash
git add examples/playground.ts .gitignore
git commit -m "chore: playground demonstrates filling a text field"
```

---

## Self-Review notes (for the controller)

- **Spec coverage:** §2 fill text ✅ (Task 1 text), radio ✅, dropdown ✅, checkbox ✅ (Button path covers checkbox + radio; checkbox uses `states[0]` on-state). Visual signature is Milestone 6, not here. Incremental save ✅ (`IncrementalDocument`). Full-rewrite `save({incremental:false})` is **deferred** — not required to pass Milestone 3 and adds no fixture value yet (corpus is classic-xref); add when a use case appears.
- **Out of scope (carried to M4):** appearance streams. Filled values rely on `/NeedAppearances true` (true for the whole corpus) so viewers render them; tests assert on `/V`, which is appearance-independent.
- **Type consistency:** `FillOp {name,value}` identical in Rust (`fill.rs`) and TS (`fields.ts`). `FillQueue.toJSON()` emits the array Rust `serde_json::from_str::<Vec<FillOp>>` expects.
- **Checkbox note:** no checkbox test in the Rust suite because the de-risked fixture's buttons are radios; the checkbox path is the *same* `Button` code. If a checkbox fixture field is available, the implementer may add one test, but it is not required to pass.
