# Milestone 25 — Form-Field Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Create AcroForm fields (text, checkbox, radio, dropdown, list box, signature) on documents built with `PdfDocument.create()`, via a type-accumulating `doc.createForm()` builder, with generated appearances so fields render and remain fillable/flattenable.

**Architecture:** `create_document` (Rust) gains a `fields_json` input. When present it emits an `/AcroForm` catalog entry (`/Fields`, `/DR/Font/Helv`, `/DA`, `/NeedAppearances false`), one field dict per field, widget annotations linked to pages via `/Annots` + `/P`, and appearance streams (text/choice via the existing `build_appearance_xobject`; buttons via generated vector appearances). TS side: a builder in `src/generate/form-builder.ts` accumulates a `FormSchema` `S` and field definitions on the document; `save()` serializes them to `fields_json`. The result is typed by the existing `TypedPdfForm<S>` machinery from `./forms`.

**Tech Stack:** Rust (lopdf 0.41), wasm-bindgen, TypeScript ESM, bun test.

**Spec:** `docs/superpowers/specs/2026-06-12-pdf-generation-design.md` (M25 addendum).

**Environment:** `source "$HOME/.cargo/env"`; `bun run build:wasm` after Rust changes. Baselines after M24 merge: cargo 83 pass, bun 91 pass / 4 skip / 0 fail, typecheck clean.

**Ground-truth dictionary shapes** (verified against the read path in forms.rs/fill.rs/flatten.rs):
- AcroForm (on catalog): `{ Fields: [refs], DR: { Font: { Helv: ref } }, DA: "/Helv 0 Tf 0 g", NeedAppearances: false }`.
- `/DR/Font/Helv` font dict: reuse `crate::draw::font_dict("Helvetica")` → `{Type:Font,Subtype:Type1,BaseFont:Helvetica,Encoding:WinAnsiEncoding}`.
- Text field: `{ FT:/Tx, T:(name), V:(value), Ff:int, MaxLen:int?, DA:str?, Rect:[x0,y0,x1,y1], AP:{N:ref}, P:pageRef, Type:/Annot, Subtype:/Widget }` (single-widget = field dict IS the widget).
- Checkbox: `{ FT:/Btn, T, V:/On|/Off, AS:/On|/Off, AP:{N:{On:ref,Off:ref}}, Rect, P, Type/Subtype }`.
- Radio group: parent `{ FT:/Btn, Ff:(1<<15), T, V:/<sel>|/Off, Kids:[widgetRefs] }`; each kid `{ Subtype:/Widget, Rect, P, Parent:parentRef, AS:/<value>|/Off, AP:{N:{<value>:ref, Off:ref}} }`.
- Dropdown: `{ FT:/Ch, Ff:(1<<17), T, Opt:[...], V:(sel), I:[idx], DA, Rect, AP:{N:ref}, P, Type/Subtype }`. List box: same without the combo bit.
- Signature: `{ FT:/Sig, T, Rect, AP:{N:ref}?, P, Type:/Annot, Subtype:/Widget }`.
- Widget→page: append the widget's ObjectId to the page's `/Annots` array AND set `/P` on the widget to the page ref.
- `/Ff` bits: readOnly = 1<<0, required = 1<<1, multiline (Tx) = 1<<12, radio = 1<<15, pushbutton = 1<<16, combo (Ch) = 1<<17.
- `/MK` (appearance characteristics) for border/background: `{ BC:[r g b], BG:[r g b] }`; border width via `/BS { W:int, S:/S }`.

**Reuse helpers:** `crate::draw::font_dict`, `appearance::{build_appearance_xobject, text_appearance_content, helvetica_widths, standard_14_widths, escape_pdf_literal, encode_winansi}`, `crate::draw::fmt_num`. create.rs already builds catalog/pages and reserves ids.

---

### Task 1: Rust — AcroForm scaffolding + text fields

**Files:** Modify `crates/core/src/create.rs`, `crates/core/src/lib.rs`

- [ ] **Step 1:** Define field-def serde types in create.rs. A tagged enum keyed by `type`, camelCase:

```rust
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum FieldDef {
    Text {
        name: String, page: usize,
        x: f32, y: f32, width: f32, height: f32,
        value: Option<String>,
        #[serde(rename = "maxLength")] max_length: Option<i64>,
        multiline: Option<bool>,
        #[serde(default)] required: bool,
        #[serde(rename = "readOnly", default)] read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>, background: Option<[f32; 3]>,
    },
    // (checkbox, radio, dropdown, listbox, signature added in later tasks)
}

#[derive(Deserialize)]
struct Border { color: [f32; 3], width: f32 }
```

- [ ] **Step 2:** Change `create_document_json` signature to `(ops_json: &str, images: &[u8], fields_json: &str)`. Parse `fields_json` as `Vec<FieldDef>` (`"[]"` ⇒ empty). Validate up front: every field's `page` in range; `width`/`height` finite and > 0; `x`/`y` finite; field `name` non-empty and unique (Err on duplicate). For text, `max_length` if present >= 0.

- [ ] **Step 3:** After building pages (you have `page_id` per page — keep them in a `Vec<ObjectId>` indexed by page), if `fields` non-empty:
  1. Reserve and build a `/DR` with one Helvetica font: `let helv = doc.add_object(Object::Dictionary(crate::draw::font_dict("Helvetica")));` then `DR = { Font: { Helv: helv } }`.
  2. For each text field, build a single-widget field dict (field IS the widget). Compute the value appearance with the existing engine:
     - `let widths = appearance::helvetica_widths();`
     - `let content = appearance::text_appearance_content(&appearance::encode_winansi(value), size, width, height, 0 /*q*/, "0 g", "Helv", &widths);` (size: pick 12.0 default, or auto — use 12.0)
     - `let ap_stream = appearance::build_appearance_xobject(content, width, height, "Helv", helv);`
     - `let ap_id = doc.add_object(Object::Stream(ap_stream));`
     - field/widget dict:
       ```
       { Type:/Annot, Subtype:/Widget, FT:/Tx, T:(name), Rect:[x,y,x+width,y+height],
         DA:"/Helv 12 Tf 0 g", V:(value or ""), Ff:(flags), MaxLen:(maxLen if set),
         AP:{ N: ap_id }, P: page_ref, MK:{...if border/background} }
       ```
       (Use a string for `T`, `Object::string_literal`. `V` is a text string.)
     - flags: `(read_only as i64)<<0 | (required as i64)<<1 | (multiline as i64)<<12`.
     - append widget id to that page's `/Annots` array (create the array on the page if absent — the page dict was built in this same function, so add `/Annots` there).
     - collect the field id into the AcroForm `/Fields` array.
  3. Build the AcroForm dict `{ Fields:[ids], DR:DR, DA:"/Helv 0 Tf 0 g", NeedAppearances: false }`, add it, and set `catalog["AcroForm"] = ref`.
  Note: the page dicts are added to the doc *before* you know widget ids in some orderings — restructure so you create field/widget objects, then set the page `/Annots`. Easiest: keep `page_annots: Vec<Vec<Object>>` while building, create widgets after pages exist (pages need ids first for `/P`), then before finalizing each page, set its `/Annots`. Since create.rs adds the page dict object and you have its id, you can `doc.get_object_mut(page_id)` to add `/Annots` after creating widgets. Use whichever lopdf supports (get_object_mut exists).

- [ ] **Step 4:** lib.rs — change `create_document` export to `(ops_json: &str, images: &[u8], fields_json: &str)`, forward. Update `fuzz_api` re-export + the `create_document` fuzz target to pass `""` or `"[]"` for fields.

- [ ] **Step 5: tests** in create.rs (reuse Document::load_mem reload; assert via `crate::forms::read_fields_json` that the field round-trips):

```rust
#[test]
fn creates_text_field() {
    let fields = r#"[{"type":"text","name":"fullName","page":0,"x":56,"y":700,"width":200,"height":20,"value":"Ada"}]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    // AcroForm present with one field
    let cat = doc.catalog().unwrap();
    assert!(cat.has(b"AcroForm"));
    // forms reader sees it
    let json = crate::forms::read_fields_json(&out).unwrap();
    assert!(json.contains("fullName"));
    assert!(json.contains("\"type\":\"text\""));
    assert!(json.contains("Ada"));
}

#[test]
fn text_field_on_page_annots() {
    let fields = r#"[{"type":"text","name":"a","page":0,"x":10,"y":10,"width":100,"height":20}]"#;
    let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let page = doc.get_dictionary(pid).unwrap();
    assert!(page.get(b"Annots").unwrap().as_array().unwrap().len() == 1);
}

#[test]
fn rejects_duplicate_field_name() {
    let fields = r#"[{"type":"text","name":"x","page":0,"x":0,"y":0,"width":10,"height":10},{"type":"text","name":"x","page":0,"x":0,"y":40,"width":10,"height":10}]"#;
    assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).is_err());
}

#[test]
fn rejects_field_bad_page() {
    let fields = r#"[{"type":"text","name":"x","page":5,"x":0,"y":0,"width":10,"height":10}]"#;
    assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).is_err());
}
```

(check the exact name of the forms reader fn — likely `crate::forms::read_fields_json`; confirm and use it. Update the existing create.rs tests + fuzz to the new 3-arg signature.)

- [ ] **Step 6:** `cargo test` all pass. **Step 7: commit** `feat(core): emit AcroForm + text fields in create_document`

---

### Task 2: Rust — checkbox + radio fields (vector appearances)

**Files:** Modify `crates/core/src/create.rs` (+ a small appearance helper if cleaner)

Buttons need generated on/off appearance Form XObjects (the fill engine does not draw button appearances). Use vector paths so no extra font is needed.

- [ ] **Step 1:** Add `CheckBox` and `RadioGroup` variants to `FieldDef`:

```rust
    CheckBox {
        name: String, page: usize, x: f32, y: f32, size: f32,
        #[serde(default)] checked: bool,
        #[serde(rename = "onValue")] on_value: Option<String>, // default "Yes"
        #[serde(default)] required: bool,
        #[serde(rename = "readOnly", default)] read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>, background: Option<[f32; 3]>,
    },
    RadioGroup {
        name: String,
        selected: Option<String>,
        #[serde(default)] required: bool,
        #[serde(rename = "readOnly", default)] read_only: bool,
        tooltip: Option<String>,
        options: Vec<RadioOption>,
    },
```
with `struct RadioOption { value: String, page: usize, x: f32, y: f32, size: f32 }`.

- [ ] **Step 2:** Appearance helpers (in create.rs or appearance.rs). Each returns a Form XObject `Stream` with `BBox [0 0 size size]`:
  - `button_off_appearance(size)` — empty content (just `BBox`), or an optional border drawn from MK (keep empty; MK handles border).
  - `checkbox_on_appearance(size)` — a check/cross drawn with line paths, e.g. a tick: `q 0 g <w> w  m/l strokes  S Q`. Pick a thickness ~ size*0.1, draw a check using 2 line segments inside the box with padding.
  - `radio_on_appearance(size)` — a filled dot: `q 0 g <cx cy r> circle (4 bezier) f Q` centered, radius ~ size*0.3. Reuse the ellipse bezier math from `crate::draw` if exposed, else inline 4 `c` curves.
  Build the Form XObject with a small local helper mirroring `build_appearance_xobject` (Type/Form/FormType/BBox/empty-or-no Resources). No Font resource needed for vector content.

- [ ] **Step 3: checkbox emission.** Single-widget field:
  ```
  on = on_value.unwrap_or("Yes")
  off_ap = add_object(button_off_appearance(size))
  on_ap  = add_object(checkbox_on_appearance(size))
  AP = { N: { <on>: on_ap, Off: off_ap } }
  AS = if checked { /<on> } else { /Off }
  V  = same as AS
  { Type:/Annot, Subtype:/Widget, FT:/Btn, T:name, Rect:[x,y,x+size,y+size],
    AP, AS, V, Ff:(flags), P:page_ref, MK:{...} }
  ```
  Append to page `/Annots`, add to `/Fields`.

- [ ] **Step 4: radio emission.** Parent field (no Rect, has Kids) + one widget per option:
  ```
  parent_id = reserve (doc.new_object_id())
  for opt in options:
    off_ap = add_object(button_off_appearance(opt.size))
    on_ap  = add_object(radio_on_appearance(opt.size))
    kid = { Subtype:/Widget, Type:/Annot, Rect:[..], Parent:parent_id, P:page_ref,
            AP:{ N:{ <opt.value>: on_ap, Off: off_ap } },
            AS: if selected==Some(opt.value) {/<value>} else {/Off} }
    kid_id = add_object(kid); append kid_id to that page's /Annots; collect kid_id
  parent = { FT:/Btn, Ff:(1<<15 | flags), T:name, Kids:[kids],
             V: selected ? /<sel> : /Off }
  doc.set_object(parent_id, parent); add parent_id to /Fields
  ```
  Validate: radio `options` non-empty; `selected` (if set) matches one option value (Err otherwise); option values unique within the group.

- [ ] **Step 5: tests:**
  - `creates_checkbox_checked`: checkbox checked=true onValue default → reload; read_fields shows type "checkbox", value/state present; page Annots has the widget; AP/N has Yes + Off.
  - `creates_checkbox_custom_on_value`: onValue "On" → state "On" present.
  - `creates_radio_group`: 2 options, selected second → read_fields shows type "radio", states include both option values; parent has Kids len 2; V = selected.
  - `radio_rejects_unknown_selected` and `radio_rejects_empty_options`.
- [ ] **Step 6:** `cargo test` all pass. **Step 7: commit** `feat(core): emit checkbox and radio fields with vector appearances`

---

### Task 3: Rust — choice (dropdown/listbox) + signature fields

**Files:** Modify `crates/core/src/create.rs`

- [ ] **Step 1:** Add variants:

```rust
    Choice {
        name: String, page: usize, x: f32, y: f32, width: f32, height: f32,
        #[serde(default)] combo: bool,            // true=dropdown, false=listbox
        options: Vec<String>,
        selected: Option<String>,
        #[serde(default)] required: bool,
        #[serde(rename = "readOnly", default)] read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>, background: Option<[f32; 3]>,
    },
    Signature {
        name: String, page: usize, x: f32, y: f32, width: f32, height: f32,
        #[serde(default)] required: bool,
        #[serde(rename = "readOnly", default)] read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>, background: Option<[f32; 3]>,
    },
```
(The TS `addDropdown`/`addListBox` both map to `Choice` with `combo` true/false.)

- [ ] **Step 2: choice emission.** Single-widget:
  - Build `/Opt` as an array of text strings from `options`.
  - Value appearance: if `selected` set, render it with `text_appearance_content` like text fields (reuse the Task 1 path); else an empty appearance.
  - Validate `selected` (if set) ∈ `options`; compute `/I` = `[index]` when selected.
  ```
  { Type:/Annot, Subtype:/Widget, FT:/Ch, T:name, Rect:[..], DA:"/Helv 12 Tf 0 g",
    Ff:( (combo?1<<17:0) | flags ), Opt:[...], V:(selected or ""), I:[idx]?,
    AP:{N:ap}, P:page_ref, MK:{...} }
  ```
  Append to `/Annots` + `/Fields`.

- [ ] **Step 3: signature emission.** Single-widget, no value:
  ```
  { Type:/Annot, Subtype:/Widget, FT:/Sig, T:name, Rect:[..], Ff:(flags), P:page_ref, MK:{...} }
  ```
  (No AP needed for an empty signature field; a viewer shows it as a sign-here box. Border via MK is the visible part.) Append to `/Annots` + `/Fields`.

- [ ] **Step 4: tests:**
  - `creates_dropdown`: combo true, options + selected → read_fields type "dropdown", options present, value = selected.
  - `creates_listbox`: combo false → type "listbox".
  - `choice_rejects_unknown_selected`.
  - `creates_signature_field`: → read_fields type "signature"; field present in /Fields; widget in page Annots.
- [ ] **Step 5:** `cargo test` all pass. **Step 6: commit** `feat(core): emit choice and signature fields`

---

### Task 4: Rust — MK border/background + shared field-flag polish

**Files:** Modify `crates/core/src/create.rs`

By now `border`/`background`/`required`/`readOnly`/`tooltip` exist on the variants. Ensure they are actually emitted (some tasks above may have stubbed them).

- [ ] **Step 1:** A helper `fn mk_dict(border: &Option<Border>, background: &Option<[f32;3]>) -> Option<Dictionary>` returning `{ BC:[r g b]?, BG:[r g b]? }` (omit when both None). Attach `/MK` to every widget when `Some`. When a border has width != 1, also set `/BS { W: width, S: /S }` on the widget.
- [ ] **Step 2:** Ensure `/TU` (tooltip) is set on the field dict for every type when `tooltip` is `Some`. Ensure readOnly/required flag bits are OR-ed into every type's `/Ff` (text, checkbox, radio parent, choice, signature).
- [ ] **Step 3: tests:**
  - `field_border_and_background`: text field with border {color,width:2} + background → widget `/MK/BC`, `/MK/BG` present; `/BS/W` == 2.
  - `field_readonly_required_flags`: text field readOnly+required → `/Ff` has bits 0 and 1 set.
  - `field_tooltip`: `/TU` present.
- [ ] **Step 4:** `cargo test` all pass. **Step 5: commit** `feat(core): field MK border/background, tooltip, and flag bits`

---

### Task 5: WASM glue — create_document gains fields_json

**Files:** Modify `src/core/wasm.ts`, `src/core/wasm-browser.ts`

- [ ] **Step 1:** `source "$HOME/.cargo/env" && bun run build:wasm`.
- [ ] **Step 2:** Update `createDocument` in both glue files to `(opsJson: string, images: Uint8Array = new Uint8Array(), fieldsJson: string = "[]"): Uint8Array` forwarding `fieldsJson` to `create_document(opsJson, images, fieldsJson)`. Keep `imageInfo`/others unchanged. (Update the import if the binding arity changed.)
- [ ] **Step 3:** `bun run typecheck && bun test` — baseline green (91 pass). The `CoreWasm` interface in document.ts must be updated to match (do it here): `createDocument(opsJson: string, images?: Uint8Array, fieldsJson?: string): Uint8Array;`. Adjust the create-mode `save()` call site to pass the existing draw payload images plus `"[]"` for fields for now (Task 6 wires real fields).
- [ ] **Step 4: commit** `feat: thread fields_json through create_document glue`

---

### Task 6: TS — createForm builder, save wiring, exports, tests

**Files:** Create `src/generate/form-builder.ts`; modify `src/core/document.ts`, `src/index.ts`, `src/index.browser.ts`, `src/generate/index.ts`; Test `tests/form-generation.test.ts`

- [ ] **Step 1: form-builder.ts.** Define the wire `FieldDef` union (mirrors Rust), the option interfaces, and a `FormBuilder<S extends FormSchema>` class. The builder holds a shared `fieldDefs: FieldDef[]` array (passed in from the document) and returns `this` re-typed on each `addX`. Sketch:

```ts
import type { FormSchema, NameOfType, OptionsOf, FieldNameOf } from "../forms/schema.js";
import type { FieldInfo } from "../forms/form.js";
import { rgb, type Color } from "./color.js";

export interface FieldBorder { color: Color; width?: number }
interface BaseFieldOptions {
  page: number; x: number; y: number;
  required?: boolean; readOnly?: boolean; tooltip?: string;
  border?: FieldBorder; background?: Color;
}
export interface TextFieldOptions extends BaseFieldOptions {
  width: number; height: number; value?: string; maxLength?: number; multiline?: boolean;
}
export interface CheckBoxOptions extends BaseFieldOptions { size: number; checked?: boolean; onValue?: string }
export interface RadioOption { value: string; page: number; x: number; y: number; size: number }
export interface RadioGroupOptions { selected?: string; required?: boolean; readOnly?: boolean; tooltip?: string; options: readonly RadioOption[] }
export interface ChoiceOptions<O extends string> extends BaseFieldOptions {
  width: number; height: number; options: readonly O[]; selected?: O;
}
export interface SignatureFieldOptions extends BaseFieldOptions { width: number; height: number }

/** @internal wire format consumed by create_document. */
export type FieldDef = /* tagged union with type:"text"|"checkbox"|"radio"|"dropdown"|"listbox"|"signature", camelCase fields matching Rust */;

export class FormBuilder<S extends FormSchema = {}> {
  /** @internal */
  constructor(private readonly defs: FieldDef[]) {}

  addTextField<N extends string>(name: N, opts: TextFieldOptions): FormBuilder<S & Record<N, { type: "text"; readOnly: boolean; value: string | null; states: []; options: [] }>> {
    this.defs.push({ type: "text", name, ...flatten(opts) });
    return this as any;
  }
  addCheckBox<N extends string>(name: N, opts: CheckBoxOptions): FormBuilder<S & Record<N, { type: "checkbox"; /*...*/ }>> { ... }
  addRadioGroup<N extends string, O extends string>(name: N, opts: RadioGroupOptions & { options: readonly (RadioOption & { value: O })[] }): FormBuilder<S & Record<N, { type: "radio"; states: O[]; /*...*/ }>> { ... }
  addDropdown<N extends string, O extends string>(name: N, opts: ChoiceOptions<O>): FormBuilder<S & Record<N, { type: "dropdown"; options: O[]; /*...*/ }>> { ... }
  addListBox<N extends string, O extends string>(name: N, opts: ChoiceOptions<O>): FormBuilder<S & Record<N, { type: "listbox"; options: O[]; /*...*/ }>> { ... }
  addSignatureField<N extends string>(name: N, opts: SignatureFieldOptions): FormBuilder<S & Record<N, { type: "signature"; /*...*/ }>> { ... }

  /** Field names declared so far (compile-time narrowed). */
  getFieldNames(): FieldNameOf<S>[] { return this.defs.map(d => d.name) as FieldNameOf<S>[]; }
}
```

  - The `FieldDef` pushed must use the exact camelCase keys Rust expects (`onValue`, `maxLength`, `readOnly`, `border:{color:[r,g,b],width}`, `background:[r,g,b]`, choice `combo` derived: dropdown ⇒ `combo:true`, listbox ⇒ `combo:false`). Convert `Color` → `[r,g,b]` and `FieldBorder` → `{color:[r,g,b],width}` when pushing.
  - Validate inputs at push time with `RangeError`: finite/positive width/height/size; non-empty unique names (track a `Set`); radio options non-empty + unique values + `selected` ∈ values; choice `selected` ∈ options; `maxLength` >= 0 if set.
  - Type accumulation: each `addX` returns `this as unknown as FormBuilder<S & Record<...>>`. Runtime mutates `this.defs`.

- [ ] **Step 2: document.ts wiring.** In `PdfDocumentBase`:
  - add `private readonly fieldDefs: FieldDef[] = [];` and import `FormBuilder`, `FieldDef`.
  - add `createForm(): FormBuilder` — only in create mode (throw `PdfError` otherwise): `return new FormBuilder(this.fieldDefs);`
  - in `save()` create-mode branch, serialize: `this.wasm.createDocument(opsJson, images, JSON.stringify(this.fieldDefs))` (opsJson/images from the existing `toCreatePayload()`).

- [ ] **Step 3: exports.** Re-export from `src/index.ts`, `src/index.browser.ts`, and `src/generate/index.ts`:
  ```ts
  export { FormBuilder } from "./generate/form-builder.js";   // path adjusted per file
  export type { TextFieldOptions, CheckBoxOptions, RadioGroupOptions, RadioOption, ChoiceOptions, SignatureFieldOptions, FieldBorder } from "./generate/form-builder.js";
  ```

- [ ] **Step 4: tests** `tests/form-generation.test.ts`. Build a doc, add one of each field type, save, reload with `PdfDocument.load`, and assert via `getForm().getFields()` that each field exists with the right `type`, value/options/states. Cases:
  - text field with value + maxLength → reloaded field type "text", value matches.
  - checkbox checked → type "checkbox", states include on-value, value is the on-value.
  - radio group 2 options selected → type "radio", states = both values, value = selected.
  - dropdown with options + selected → type "dropdown", options match, value = selected.
  - list box → type "listbox".
  - signature field → type "signature".
  - readOnly/required/tooltip reflected (readOnly via `field.readOnly === true`).
  - `createForm()` on a loaded doc throws.
  - validation: duplicate name throws; radio empty options throws; choice bad selected throws.
  - **type-level (compile) checks** in `tests/types/`: add a `form-builder.types.ts` asserting `getFieldNames()` is narrowed and choice `selected` only accepts declared options (mirror the existing `tests/types/typed-form.types.ts` style; it's compiled by typecheck).

- [ ] **Step 5:** `bun run typecheck && bun test` (91 + ~12). `bun run build:js`, import all 5 entries + assert `FormBuilder` on root + `./generate`, `bun run scripts/browser-entry-smoke.ts`. Iterate to green.
- [ ] **Step 6: commit** `feat: createForm builder for typed form-field generation`

---

### Task 7: Docs + release 0.3.0

**Files:** Modify `README.md`, `docs/migrating-from-pdf-lib.md`, `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml` (+ Cargo.lock), `examples/playground.ts`

- [ ] **Step 1: README** — add a "Creating form fields" subsection under generation: `doc.createForm().addTextField(...).addCheckBox(...)...` → `save()`, note created-docs-only, typed names, all six field types, optional border/background, and that the result is a normal fillable AcroForm (can be filled/flattened by the same library after reload). Cross-check names against the real exports.
- [ ] **Step 2: migration guide** — map pdf-lib `form.createTextField/createCheckBox/createRadioGroup/createDropdown/createOptionList` → `doc.createForm().addTextField/addCheckBox/addRadioGroup/addDropdown/addListBox`; note created-docs-only and typed accumulation.
- [ ] **Step 3: CHANGELOG** — new `## [0.3.0] - 2026-06-13` with **Added**: `createForm()` builder; text/checkbox/radio/dropdown/listbox/signature field creation on generated docs; per-field value/readOnly/required/tooltip/maxLength/multiline; border/background; typed field-name accumulation.
- [ ] **Step 4: version bump** — `package.json` and `crates/core/Cargo.toml` to `0.3.0`; refresh Cargo.lock entry.
- [ ] **Step 5: playground** — add a short "Generate a fillable form" section to `examples/playground.ts` (create → addPage → createForm with a couple of fields → save → reload → list fields). Keep the guided-tour style. Run `bun run play` to confirm.
- [ ] **Step 6:** `bun run typecheck && bun test && source "$HOME/.cargo/env" && cargo test --manifest-path crates/core/Cargo.toml && bun run build:js` — all green.
- [ ] **Step 7: commit** `docs: document form generation; release 0.3.0`

---

### Final verification (whole milestone)

- [ ] `cargo test` 0 fail; typecheck clean; `bun test` 0 fail.
- [ ] All 5 export entries resolve; `FormBuilder` on root + `./generate`; browser smoke passes.
- [ ] Round-trip: a created doc with all six field types reloads via `PdfDocument.load`, `getForm().getFields()` returns them with correct types, and a text field can then be filled + flattened (compose with the existing fill path).
- [ ] `npm pack --dry-run` (with cargo env) ships `dist/generate/*` + the rebuilt wasm.
