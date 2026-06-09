# Milestone 2: Parse + Read AcroForm Fields — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read the AcroForm of a loaded PDF and expose every field's name, type, current value, and valid states/options through a typed TypeScript API (`doc.getForm().getFields()`), verified against the real fixture corpus.

**Architecture:** The Rust core parses the PDF with `lopdf`, walks the AcroForm `Fields` tree, and returns a JSON array of field descriptors across the coarse WASM boundary (`read_fields(bytes) -> json string`). The TypeScript layer parses that JSON into typed `FieldInfo` objects exposed via a `PdfForm` class. Read-only — no mutation yet.

**Tech Stack:** Rust + `lopdf 0.41` (PDF parsing) + `serde`/`serde_json` (boundary serialization), `wasm-bindgen 0.2.123`, TypeScript, `bun`.

**Grounding (already validated during planning):**
- `lopdf 0.41` parses all fixtures (PDF 1.3, classic xref). Confirmed field counts (54 / 30 / 109) and extraction of radio on-states (`["Titular","Familiar"]`, `["Si","No"]`) from widget `/AP/N` and dropdown options (`["Soltero","Casado",...]`) from `/Opt`.
- lopdf's encryption module pulls a non-optional `getrandom`, which fails on `wasm32-unknown-unknown` unless lopdf's `wasm_js` feature is enabled. The wasm build succeeds with the target-specific dependency below; `serde`/`serde_json` are pure-Rust and compile to wasm cleanly.

---

## File Structure

- `crates/core/Cargo.toml` — add `serde`, `serde_json`; lopdf dependency already present (validated config restated in Task 1).
- `crates/core/src/lib.rs` — add the `read_fields` WASM export (keep existing `round_trip`). Delegates to a new `forms` module.
- `crates/core/src/forms.rs` — new: AcroForm traversal + `FieldInfo` model + JSON serialization. One responsibility: turning PDF bytes into field descriptors.
- `src/wasm.ts` — add a `readFields(data) => string` wrapper alongside `roundTrip`.
- `src/form.ts` — new: `PdfForm` class + `FieldInfo`/`FieldType` types (the public field API).
- `src/index.ts` — add `PdfDocument.getForm()` returning a `PdfForm`.
- `tests/fields.test.ts` — new: TS-level field-reading tests against fixtures.
- `examples/playground.ts` — extend to print the fields it finds.

> Keep `crates/core/src/forms.rs` focused on traversal/serialization only. If it grows past field reading (e.g. mutation), that belongs in Milestone 3.

---

## Field model (the contract both sides share)

`read_fields` returns a JSON array. Each element:

```jsonc
{
  "name": "beneficiario.tipo_beneficiario", // fully-qualified name (ancestor /T joined by ".")
  "type": "radio",            // see FieldType below
  "value": "Off",             // current /V as a plain string, or null if absent
  "states": ["Titular","Familiar"], // on-states for checkbox/radio (from widget /AP/N, minus "Off"); [] otherwise
  "options": [],              // option export values for dropdown/listbox (from /Opt); [] otherwise
  "readOnly": false           // Ff bit 1
}
```

`FieldType` (string union): `"text" | "checkbox" | "radio" | "dropdown" | "listbox" | "signature" | "pushbutton" | "unknown"`.

**Type derivation** from `/FT` (inheritable from `/Parent`) + `/Ff` flag bits (PDF 1-based bit *n* = value `1 << (n-1)`):
- `/Tx` → `text`
- `/Btn` → `pushbutton` if `Ff & (1<<16)` (65536); else `radio` if `Ff & (1<<15)` (32768); else `checkbox`
- `/Ch` → `dropdown` if `Ff & (1<<17)` (131072); else `listbox`
- `/Sig` → `signature`
- otherwise → `unknown`

`readOnly` = `Ff & 1`.

---

## Task 1: Dependencies (serde + validated lopdf/wasm config)

**Files:**
- Modify: `crates/core/Cargo.toml`

- [ ] **Step 1: Set the full `[dependencies]` and wasm target sections of `crates/core/Cargo.toml`**

The `[package]` and `[lib]` sections stay as they are. Ensure the dependency sections read exactly:

```toml
[dependencies]
wasm-bindgen = "0.2.123"
lopdf = { version = "0.41", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# On wasm32, lopdf's (non-optional) getrandom dependency needs its JS backend.
[target.'cfg(target_arch = "wasm32")'.dependencies]
lopdf = { version = "0.41", default-features = false, features = ["wasm_js"] }
```

- [ ] **Step 2: Verify native build resolves**

Run: `cargo build --manifest-path crates/core/Cargo.toml`
Expected: compiles; exits 0.

- [ ] **Step 3: Commit**

```bash
git add crates/core/Cargo.toml crates/core/Cargo.lock
git commit -m "build(core): add lopdf, serde for AcroForm reading"
```

---

## Task 2: Rust field model + traversal (TDD with fixtures)

**Files:**
- Create: `crates/core/src/forms.rs`
- Modify: `crates/core/src/lib.rs` (add `mod forms;` and the `read_fields` export)
- Test: inline `#[cfg(test)]` module in `crates/core/src/forms.rs` using `include_bytes!` on fixtures

- [ ] **Step 1: Write the failing tests in `crates/core/src/forms.rs`**

Create the file with ONLY this test module first (the items it references don't exist yet, so it won't compile — that is the red state):

```rust
#[cfg(test)]
mod tests {
    use super::read_fields_json;

    fn fields(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_str(&read_fields_json(bytes).unwrap()).unwrap()
    }

    const VIAJERO: &[u8] =
        include_bytes!("../../../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf");
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn reads_all_text_fields_of_viajero() {
        let f = fields(VIAJERO);
        assert_eq!(f.as_array().unwrap().len(), 54);
        assert_eq!(f[0]["name"], "viajero.destino");
        assert_eq!(f[0]["type"], "text");
    }

    #[test]
    fn classifies_radio_with_export_states() {
        let f = fields(FICHA);
        let radio = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "beneficiario.tipo_beneficiario")
            .unwrap();
        assert_eq!(radio["type"], "radio");
        let states: Vec<&str> = radio["states"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(states.contains(&"Titular") && states.contains(&"Familiar"));
    }

    #[test]
    fn classifies_dropdown_with_options() {
        let f = fields(FICHA);
        let dd = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "beneficiario.estado_civil")
            .unwrap();
        assert_eq!(dd["type"], "dropdown");
        let opts: Vec<&str> = dd["options"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(opts.contains(&"Soltero"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile (red)**

Run: `cargo test --manifest-path crates/core/Cargo.toml forms`
Expected: FAIL — `cannot find function read_fields_json in module super`.

- [ ] **Step 3: Implement the field reader above the test module in `crates/core/src/forms.rs`**

```rust
use lopdf::{Dictionary, Document, Object};
use serde::Serialize;

#[derive(Serialize)]
pub struct FieldInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub value: Option<String>,
    pub states: Vec<String>,
    pub options: Vec<String>,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
}

/// Parse `data` and return its AcroForm fields as a JSON array string.
pub fn read_fields_json(data: &[u8]) -> Result<String, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let fields = collect_fields(&doc).map_err(|e| e.to_string())?;
    serde_json::to_string(&fields).map_err(|e| e.to_string())
}

fn collect_fields(doc: &Document) -> Result<Vec<FieldInfo>, String> {
    let catalog = as_dict(doc, doc.trailer.get(b"Root").map_err(|e| e.to_string())?)?;
    let acroform = match catalog.get(b"AcroForm") {
        Ok(o) => as_dict(doc, o)?,
        Err(_) => return Ok(Vec::new()), // no form
    };
    let entries = acroform
        .get(b"Fields")
        .and_then(|o| o.as_array())
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for entry in entries {
        let d = as_dict(doc, entry)?;
        out.push(describe_field(doc, d));
    }
    Ok(out)
}

fn describe_field(doc: &Document, d: &Dictionary) -> FieldInfo {
    let name = fully_qualified_name(doc, d);
    let ft = inherited_name(doc, d, b"FT").unwrap_or_default();
    let ff = inherited_int(doc, d, b"Ff").unwrap_or(0);
    let field_type = classify(&ft, ff).to_string();

    let value = d.get(b"V").ok().and_then(value_to_string);

    let mut states = Vec::new();
    collect_on_states(doc, d, &mut states);
    if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kd) = as_dict(doc, k) {
                collect_on_states(doc, kd, &mut states);
            }
        }
    }

    let options = d
        .get(b"Opt")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().map(opt_export).collect())
        .unwrap_or_default();

    FieldInfo {
        name,
        field_type,
        value,
        states,
        options,
        read_only: ff & 1 != 0,
    }
}

fn classify(ft: &str, ff: i64) -> &'static str {
    match ft {
        "Tx" => "text",
        "Btn" => {
            if ff & (1 << 16) != 0 {
                "pushbutton"
            } else if ff & (1 << 15) != 0 {
                "radio"
            } else {
                "checkbox"
            }
        }
        "Ch" => {
            if ff & (1 << 17) != 0 {
                "dropdown"
            } else {
                "listbox"
            }
        }
        "Sig" => "signature",
        _ => "unknown",
    }
}

// --- helpers ---

// AcroForm/field/widget objects may be inline dictionaries OR indirect references.
fn as_dict<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Dictionary, String> {
    match o {
        Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string()),
        Object::Dictionary(d) => Ok(d),
        other => Err(format!("expected dict/ref, got {:?}", other)),
    }
}

fn name_part(doc: &Document, d: &Dictionary) -> Option<String> {
    d.get(b"T").ok().and_then(|o| o.as_str().ok()).map(|s| String::from_utf8_lossy(s).into_owned())
}

fn fully_qualified_name(doc: &Document, d: &Dictionary) -> String {
    // Walk up the /Parent chain, collecting /T parts, then join root..leaf with ".".
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = name_part(doc, d) {
        parts.push(p);
    }
    let mut cur = d;
    while let Ok(parent) = cur.get(b"Parent").and_then(|o| {
        if let Object::Reference(id) = o {
            doc.get_dictionary(*id)
        } else if let Object::Dictionary(pd) = o {
            Ok(pd)
        } else {
            Err(lopdf::Error::Type)
        }
    }) {
        if let Some(p) = name_part(doc, parent) {
            parts.push(p);
        }
        cur = parent;
    }
    parts.reverse();
    parts.join(".")
}

fn inherited_name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    inherited(doc, d, key).and_then(|o| o.as_name().ok().map(|n| String::from_utf8_lossy(n).into_owned()))
}

fn inherited_int(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<i64> {
    inherited(doc, d, key).and_then(|o| o.as_i64().ok())
}

fn inherited<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    if let Ok(o) = d.get(key) {
        return Some(o);
    }
    let mut cur = d;
    while let Ok(parent) = cur.get(b"Parent") {
        let pd = as_dict(doc, parent).ok()?;
        if let Ok(o) = pd.get(key) {
            return Some(o);
        }
        cur = pd;
    }
    None
}

fn value_to_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::String(s, _) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

// Push a widget's /AP /N appearance-state keys (minus "Off") into `out`.
fn collect_on_states(doc: &Document, widget: &Dictionary, out: &mut Vec<String>) {
    let Some(ap) = widget.get(b"AP").ok().and_then(|o| as_dict(doc, o).ok()) else { return };
    let Some(n) = ap.get(b"N").ok().and_then(|o| as_dict(doc, o).ok()) else { return };
    for (k, _) in n.iter() {
        let s = String::from_utf8_lossy(k).into_owned();
        if s != "Off" && !out.contains(&s) {
            out.push(s);
        }
    }
}

// A /Opt entry is either a string, or a [export_value, display_text] pair; take the export value.
fn opt_export(o: &Object) -> String {
    match o {
        Object::Array(a) => a.first().and_then(value_to_string).unwrap_or_default(),
        other => value_to_string(other).unwrap_or_default(),
    }
}
```

> Note on the lopdf error type in `fully_qualified_name`: if `lopdf::Error::Type` does not exist by that exact name in 0.41, replace the `Err(...)` arm with any lopdf error constructor that compiles (e.g. `Err(lopdf::Error::ObjectNotFound)` or restructure with `as_dict`). The behavior required is only "stop walking when /Parent is neither a reference nor a dict".

- [ ] **Step 4: Wire the module into `crates/core/src/lib.rs`**

Add at the top of `lib.rs` (keep the existing `round_trip`):

```rust
mod forms;

/// Read the AcroForm fields of a PDF, returned as a JSON array string.
#[wasm_bindgen]
pub fn read_fields(data: &[u8]) -> Result<String, JsError> {
    forms::read_fields_json(data).map_err(|e| JsError::new(&e))
}
```

Ensure the `use wasm_bindgen::prelude::*;` line at the top of `lib.rs` is present (it already is from Milestone 1) so `JsError` resolves.

- [ ] **Step 5: Run the tests to verify they pass (green)**

Run: `cargo test --manifest-path crates/core/Cargo.toml`
Expected: PASS — `reads_all_text_fields_of_viajero`, `classifies_radio_with_export_states`, `classifies_dropdown_with_options`, plus the existing `round_trip` test, all ok.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/forms.rs crates/core/src/lib.rs
git commit -m "feat(core): read AcroForm fields (name/type/value/states/options)"
```

---

## Task 3: Rebuild WASM + TS loader wrapper

**Files:**
- Modify: `src/wasm.ts`
- Regenerates: `pkg/` (gitignored)

- [ ] **Step 1: Rebuild the WASM package**

Run: `bun run build:wasm`
Expected: exits 0; `grep -c read_fields pkg/better_pdf_core.d.ts` prints ≥ 1.

- [ ] **Step 2: Add the wrapper to `src/wasm.ts`** (keep the existing `roundTrip`)

```ts
export function readFields(data: Uint8Array): string {
  return core.read_fields(data);
}
```

- [ ] **Step 3: Commit**

```bash
git add src/wasm.ts
git commit -m "feat: wasm loader exposes readFields"
```

---

## Task 4: TypeScript `PdfForm` API (TDD)

**Files:**
- Create: `src/form.ts`
- Modify: `src/index.ts`
- Test: `tests/fields.test.ts`

- [ ] **Step 1: Write the failing test in `tests/fields.test.ts`**

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

async function form() {
  const doc = await PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
  return doc.getForm();
}

test("getFields returns all fields with names and types", async () => {
  const fields = (await form()).getFields();
  expect(fields.length).toBe(30);
  expect(fields[0]!.name).toBe("beneficiario.apellidos_nombres");
  expect(fields[0]!.type).toBe("text");
});

test("radio field exposes its export states", async () => {
  const f = (await form()).getField("beneficiario.tipo_beneficiario");
  expect(f?.type).toBe("radio");
  expect(f?.states).toEqual(expect.arrayContaining(["Titular", "Familiar"]));
});

test("dropdown field exposes its options", async () => {
  const f = (await form()).getField("beneficiario.estado_civil");
  expect(f?.type).toBe("dropdown");
  expect(f?.options).toEqual(expect.arrayContaining(["Soltero"]));
});

test("getField returns undefined for an unknown name", async () => {
  expect((await form()).getField("does.not.exist")).toBeUndefined();
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test tests/fields.test.ts`
Expected: FAIL — `doc.getForm is not a function`.

- [ ] **Step 3: Implement `src/form.ts`**

```ts
import { readFields } from "./wasm.ts";

export type FieldType =
  | "text"
  | "checkbox"
  | "radio"
  | "dropdown"
  | "listbox"
  | "signature"
  | "pushbutton"
  | "unknown";

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

/** Read-only view over a PDF's AcroForm fields. */
export class PdfForm {
  private readonly fields: FieldInfo[];

  /** @internal */
  constructor(bytes: Uint8Array) {
    this.fields = JSON.parse(readFields(bytes)) as FieldInfo[];
  }

  /** All fields in document order. */
  getFields(): FieldInfo[] {
    return this.fields;
  }

  /** A single field by its fully-qualified name, or undefined if absent. */
  getField(name: string): FieldInfo | undefined {
    return this.fields.find((f) => f.name === name);
  }
}
```

- [ ] **Step 4: Add `getForm()` to `src/index.ts`**

Add the import at the top:

```ts
import { PdfForm } from "./form.ts";
```

And add this method to the `PdfDocument` class (alongside `save`):

```ts
  /** Read the document's AcroForm fields. */
  getForm(): PdfForm {
    return new PdfForm(this.bytes);
  }
```

Also re-export the types for consumers — add at the bottom of `src/index.ts`:

```ts
export { PdfForm } from "./form.ts";
export type { FieldInfo, FieldType } from "./form.ts";
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bun test tests/fields.test.ts`
Expected: PASS — all four tests green.

- [ ] **Step 6: Run the full suite + typecheck**

Run: `bun test && bunx tsc --noEmit`
Expected: all tests pass; `tsc` exits 0.

- [ ] **Step 7: Commit**

```bash
git add src/form.ts src/index.ts tests/fields.test.ts
git commit -m "feat: PdfForm.getFields/getField typed API"
```

---

## Task 5: Extend the playground to list fields

**Files:**
- Modify: `examples/playground.ts`

- [ ] **Step 1: After the round-trip block in `examples/playground.ts`, add a field listing**

Append before the end of the file:

```ts
const form = doc.getForm();
const fields = form.getFields();
console.log(`\nAcroForm fields: ${fields.length}`);
for (const f of fields.slice(0, 15)) {
  const extra =
    f.states.length ? ` states=${JSON.stringify(f.states)}` :
    f.options.length ? ` options=${JSON.stringify(f.options)}` : "";
  console.log(`  ${f.type.padEnd(10)} ${f.name}${extra}`);
}
if (fields.length > 15) console.log(`  ... and ${fields.length - 15} more`);
```

- [ ] **Step 2: Run it**

Run: `bun run play "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf"`
Expected: prints the round-trip line AND `AcroForm fields: 30` followed by typed field rows, including the `radio` row with `states=["Titular","Familiar"]` and the `dropdown` row with its options.

- [ ] **Step 3: Commit**

```bash
git add examples/playground.ts
git commit -m "chore: playground lists AcroForm fields"
```

---

## Self-Review

**Spec coverage (Milestone 2 slice of the design spec §2/§4/§5):**
- Read AcroForm fields with name, type, value — Task 2 (`describe_field`), Task 4 (`FieldInfo`). ✅
- Expose valid states (radio/checkbox) and options (dropdown) using *real* export values, never assumed `/Yes` — Task 2 (`collect_on_states`, `opt_export`), verified by tests in Tasks 2 and 4. ✅
- Classic xref + object-stream PDFs handled — delegated to `lopdf` (validated on the PDF 1.3 corpus). ✅
- Coarse WASM boundary (bytes in → JSON out) — Task 2/3. ✅
- Typed TS API matching spec §5 (`getForm`, `getFields`, `getField`) — Task 4. ✅
- Out of scope (correctly deferred to later milestones): mutation, appearances, flatten, signatures.

**Placeholder scan:** No TBD/TODO; every step shows complete code/commands. The one conditional note (lopdf error-variant name in `fully_qualified_name`) gives an explicit, compilable fallback — not a placeholder. ✅

**Type consistency:** Rust `FieldInfo` serde field renames (`type`, `readOnly`) match the TS `FieldInfo` interface keys; `read_fields` (Rust/snake) → `read_fields` (wasm export) → `readFields` (`src/wasm.ts`) → consumed by `PdfForm`. `FieldType` string union matches the strings returned by `classify`. ✅
