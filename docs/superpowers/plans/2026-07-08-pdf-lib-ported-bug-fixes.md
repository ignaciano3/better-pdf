# pdf-lib Ported-Test Bug Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make all 65 tests in `tests/pdf-lib-ported.test.ts` pass by fixing the 6 bugs they exposed (UTF-16 field names, unresolved indirect `/V`//`Opt`, radio `/Opt` mapping, and parser robustness for broken xref/trailer/root/endstream files).

**Architecture:** All fixes live in the Rust core (`crates/core/src`); the TS wrapper needs no API changes. Bugs 1–3 are surgical fixes in `forms.rs`/`fill.rs` (decode strings with lopdf's `decode_text_string`, dereference indirect objects, map radio on-states through `/Opt`). Bugs 4–6 share one root cause — lopdf's strict parser rejects files pdf-lib repairs — fixed by a new `repair.rs` module that reconstructs a well-formed PDF byte stream when `Document::load_mem` fails, wired in as a fallback inside `doc_io::load_pdf`.

**Tech Stack:** Rust (lopdf 0.41, wasm-bindgen), TypeScript wrapper, `bun test` for e2e, `cargo test -p better-pdf-core` for unit tests. Rebuild wasm with `bun run build:wasm`.

## Global Constraints

- CI gates `cargo clippy`, NOT `cargo fmt` — do not reformat files you aren't otherwise changing (user preference).
- Do not split or restructure the public `PdfDocument` TS type (user preference).
- The repair path must run ONLY when the normal `Document::load_mem` fails — the happy load→mutate→save path must not get slower (user preference: benchmark-sensitive hot path; run `bun run bench` before/after Task 1 and confirm no regression).
- Verification for every task: `cargo test -p better-pdf-core` for Rust units, then `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts` for the e2e evidence. The full suite `bun test` must pass before the final commit.
- Test fixtures from pdf-lib live in `tests/fixtures/pdf-lib/` (already committed to the working tree by the bug-hunt session). Rust unit tests reference them via `include_bytes!("../../../tests/fixtures/pdf-lib/<name>.pdf")`.
- Commit after each task with a `fix(core): ...` message; end commit messages with the Co-Authored-By line from the repo's convention.

---

### Task 1: `repair.rs` — reconstruct unparseable PDFs (bugs 4, 5, 6)

Fixes: `PdfCoreError: failed parsing cross reference table: invalid start value` (`missing_xref_trailer_dict.pdf`, `with_invalid_objects.pdf`, `PDF 2.0 with offset start.pdf`), `couldn't parse input: invalid file trailer` (`just_metadata.pdf`, `with_missing_endstream_eol_and_polluted_ctm.pdf`), and `getMetadata()` returning `{}` for `just_metadata.pdf` (its root cause is the failed parse — `metadata.rs::get_str` already decodes hex/UTF-16 correctly).

**Files:**
- Create: `crates/core/src/repair.rs`
- Modify: `crates/core/src/doc_io.rs:76-84` (`load_pdf`)
- Modify: `crates/core/src/lib.rs` (add `mod repair;`)

**Interfaces:**
- Produces: `pub(crate) fn repair_load(data: &[u8]) -> Result<lopdf::Document, String>` — used only by `doc_io::load_pdf`.
- `doc_io::load_pdf` keeps its exact signature `pub fn load_pdf(data: &[u8]) -> Result<Document, String>`; all callers are untouched.

**Approach (mirrors pdf-lib's recovery):** scan the raw bytes for `N G obj … endobj` spans (honoring `stream`/`endstream` so binary stream data is never scanned for keywords, and tolerating a missing `endobj` by stopping at the next object header), copy each span verbatim, and emit a rebuilt file: `%PDF-1.7` header + the object spans + a freshly computed xref table + trailer. The trailer's `/Root` comes from the last object whose dict contains `/Type /Catalog`; `/Info` from a regex-free scan of any `trailer` dict text for `/Info N G R`. Then feed the rebuilt bytes to `lopdf::Document::load_mem`. Because offsets are recomputed from scratch, junk before the `%PDF` header, broken xref tables, missing trailers, and CRLF quirks all become irrelevant.

- [ ] **Step 1: Write failing Rust tests**

Append to the new `crates/core/src/repair.rs` (write the whole file in Step 3; tests shown here so you write them first in the file skeleton with `todo!()` bodies absent — the test module is real from the start):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const JUST_METADATA: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/just_metadata.pdf");
    const MISSING_XREF: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/missing_xref_trailer_dict.pdf");
    const INVALID_OBJECTS: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/with_invalid_objects.pdf");
    const OFFSET_START: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/PDF 2.0 with offset start.pdf");
    const BAD_ENDSTREAM: &[u8] = include_bytes!(
        "../../../tests/fixtures/pdf-lib/with_missing_endstream_eol_and_polluted_ctm.pdf"
    );

    fn page_count(doc: &lopdf::Document) -> usize {
        doc.get_pages().len()
    }

    #[test]
    fn repairs_missing_xref_trailer_dict() {
        let doc = repair_load(MISSING_XREF).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_just_metadata_and_preserves_info() {
        let doc = repair_load(JUST_METADATA).unwrap();
        assert_eq!(page_count(&doc), 1);
        // /Info must survive so getMetadata works (title is a hex string).
        let info = doc.trailer.get(b"Info").unwrap();
        assert!(matches!(info, lopdf::Object::Reference(_)));
    }

    #[test]
    fn repairs_offset_start() {
        let doc = repair_load(OFFSET_START).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_invalid_objects() {
        let doc = repair_load(INVALID_OBJECTS).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_missing_endstream_eol() {
        let doc = repair_load(BAD_ENDSTREAM).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn garbage_still_fails() {
        assert!(repair_load(b"this is not a pdf at all").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p better-pdf-core repair -- --nocapture`
Expected: compile error (`repair_load` not defined) — that counts as red. After writing a `todo!()` stub, tests panic.

- [ ] **Step 3: Implement `repair.rs`**

```rust
//! Recovery loader for PDFs that lopdf's strict parser rejects (broken or
//! missing xref/trailer, junk before the %PDF header, missing endobj/endstream
//! EOLs). Mirrors pdf-lib's approach: scan the raw bytes for indirect objects,
//! re-emit them verbatim with a freshly computed xref, and parse the rebuilt
//! file. Only invoked after a normal `Document::load_mem` has already failed.

use lopdf::Document;

/// One `N G obj … endobj` span found in the raw bytes.
struct RawObj {
    num: u32,
    gen: u16,
    /// Byte range of the whole span, `N G obj` through `endobj` inclusive
    /// (or through the byte before the next object header when `endobj` is
    /// missing).
    span: std::ops::Range<usize>,
}

pub(crate) fn repair_load(data: &[u8]) -> Result<Document, String> {
    let objs = scan_objects(data);
    if objs.is_empty() {
        return Err("repair failed: no indirect objects found".to_string());
    }
    let rebuilt = rebuild(data, &objs)?;
    Document::load_mem(&rebuilt).map_err(|e| format!("repair failed: {e}"))
}

/// Find every `N G obj` header outside stream data and delimit its span.
fn scan_objects(data: &[u8]) -> Vec<RawObj> {
    let mut headers: Vec<(usize, u32, u16, usize)> = Vec::new(); // (start, num, gen, body_start)
    let mut i = 0;
    while i < data.len() {
        if let Some((num, gen, header_start, body_start)) = parse_obj_header(data, i) {
            headers.push((header_start, num, gen, body_start));
            // Skip past stream data so `endstream`/`obj` bytes inside streams
            // are never mis-detected: jump to the `endstream` keyword if the
            // body opens a stream.
            i = skip_stream(data, body_start);
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (idx, &(start, num, gen, body_start)) in headers.iter().enumerate() {
        let hard_end = headers.get(idx + 1).map(|h| h.0).unwrap_or(data.len());
        // Span ends at `endobj` if present before the next header, else there.
        let end = find_keyword(&data[body_start..hard_end], b"endobj")
            .map(|p| body_start + p + b"endobj".len())
            .unwrap_or(hard_end);
        out.push(RawObj { num, gen, span: start..end });
    }
    out
}

/// Try to parse `N G obj` beginning at a digit at `pos` preceded by a
/// delimiter/whitespace (or start of file). Returns (num, gen, header_start,
/// body_start) — where body_start is the byte after the `obj` keyword.
fn parse_obj_header(data: &[u8], pos: usize) -> Option<(u32, u16, usize, usize)> {
    if !data[pos].is_ascii_digit() {
        return None;
    }
    if pos > 0 && !is_delim_or_ws(data[pos - 1]) {
        return None;
    }
    let mut i = pos;
    let num = read_uint(data, &mut i)?;
    skip_ws(data, &mut i)?;
    let gen = read_uint(data, &mut i)?;
    skip_ws(data, &mut i)?;
    if !data[i..].starts_with(b"obj") {
        return None;
    }
    Some((num as u32, gen as u16, pos, i + 3))
}

fn is_delim_or_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0' | b'>' | b']' | b')' | b'%')
}

fn read_uint(data: &[u8], i: &mut usize) -> Option<u64> {
    let start = *i;
    while *i < data.len() && data[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == start || *i - start > 10 {
        return None;
    }
    std::str::from_utf8(&data[start..*i]).ok()?.parse().ok()
}

/// At least one whitespace byte (incl. CR/LF), then skip the rest.
fn skip_ws(data: &[u8], i: &mut usize) -> Option<()> {
    let start = *i;
    while *i < data.len() && matches!(data[*i], b' ' | b'\t' | b'\r' | b'\n' | b'\0' | b'\x0c') {
        *i += 1;
    }
    (*i > start && *i < data.len()).then_some(())
}

/// If the object body contains a `stream` keyword before `endobj`, return the
/// index just past its matching `endstream`; else return `body_start`.
/// Searching for the literal `endstream` keyword (rather than trusting
/// /Length) is what makes missing-EOL and wrong-/Length files recoverable.
fn skip_stream(data: &[u8], body_start: usize) -> usize {
    let window_end = data.len();
    let Some(rel_stream) = find_keyword(&data[body_start..window_end], b"stream") else {
        return body_start;
    };
    // `endobj` before `stream` means the stream belongs to a later object.
    if let Some(rel_endobj) = find_keyword(&data[body_start..window_end], b"endobj")
        && rel_endobj < rel_stream
    {
        return body_start;
    }
    let stream_data_start = body_start + rel_stream + b"stream".len();
    match find_keyword(&data[stream_data_start..], b"endstream") {
        Some(rel) => stream_data_start + rel + b"endstream".len(),
        None => body_start,
    }
}

/// memmem: first occurrence of `needle` in `haystack`.
fn find_keyword(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Emit header + object spans (verbatim) + xref + trailer, recomputing all
/// offsets. Root is the last /Type /Catalog object; /Info is recovered from
/// the original trailer text when present.
fn rebuild(data: &[u8], objs: &[RawObj]) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;
    // Deduplicate by object number, last occurrence wins (incremental updates).
    let mut by_num: BTreeMap<u32, &RawObj> = BTreeMap::new();
    for o in objs {
        by_num.insert(o.num, o);
    }

    let root = by_num
        .values()
        .rev()
        .find(|o| contains_outside_stream(&data[o.span.clone()], b"/Catalog"))
        .map(|o| (o.num, o.gen))
        .ok_or("repair failed: no /Type /Catalog object found")?;
    let info = find_info_ref(data, &by_num);

    let mut out: Vec<u8> = b"%PDF-1.7\n%\xC7\xEC\x8F\xA2\n".to_vec();
    let mut offsets: BTreeMap<u32, (u64, u16)> = BTreeMap::new();
    for (&num, o) in &by_num {
        offsets.insert(num, (out.len() as u64, o.gen));
        out.extend_from_slice(&data[o.span.clone()]);
        // Guarantee the span is properly terminated.
        if !out.ends_with(b"endobj") {
            out.extend_from_slice(b"\nendobj");
        }
        out.push(b'\n');
    }

    let xref_pos = out.len();
    let max_num = *offsets.keys().next_back().unwrap();
    out.extend_from_slice(format!("xref\n0 {}\n", max_num + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for num in 1..=max_num {
        match offsets.get(&num) {
            Some(&(off, gen)) => {
                out.extend_from_slice(format!("{off:010} {gen:05} n \n").as_bytes())
            }
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root {} {} R", max_num + 1, root.0, root.1).as_bytes(),
    );
    if let Some((n, g)) = info {
        out.extend_from_slice(format!(" /Info {n} {g} R").as_bytes());
    }
    out.extend_from_slice(format!(" >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes());
    Ok(out)
}

/// True when `needle` occurs in `span` before any `stream` keyword (so we
/// don't match bytes inside stream data).
fn contains_outside_stream(span: &[u8], needle: &[u8]) -> bool {
    let limit = find_keyword(span, b"stream").unwrap_or(span.len());
    find_keyword(&span[..limit], needle).is_some()
}

/// Recover `/Info N G R` from the original file's trailer text, keeping it
/// only when object N was actually found.
fn find_info_ref(
    data: &[u8],
    by_num: &std::collections::BTreeMap<u32, &RawObj>,
) -> Option<(u32, u16)> {
    let mut search_from = 0;
    let mut last: Option<(u32, u16)> = None;
    while let Some(rel) = find_keyword(&data[search_from..], b"/Info") {
        let mut i = search_from + rel + b"/Info".len();
        let _ = skip_ws(data, &mut i);
        if let Some(num) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && let Some(gen) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && data.get(i) == Some(&b'R')
            && by_num.contains_key(&(num as u32))
        {
            last = Some((num as u32, gen as u16));
        }
        search_from += rel + 1;
    }
    last
}
```

Then register the module in `crates/core/src/lib.rs` alongside the other `mod` declarations:

```rust
mod repair;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p better-pdf-core repair -- --nocapture`
Expected: all 6 tests PASS. If a specific fixture still fails, debug that fixture's byte layout with `hexdump -C` before changing strategy — do NOT weaken the test.

- [ ] **Step 5: Wire the fallback into `load_pdf`**

In `crates/core/src/doc_io.rs`, replace the body of `load_pdf` (currently lines 76–84):

```rust
pub fn load_pdf(data: &[u8]) -> Result<Document, String> {
    let doc = match Document::load_mem(data) {
        Ok(doc) => doc,
        // Strict parse failed (broken xref/trailer, junk before header, …):
        // fall back to the recovery loader. Only this error path pays the
        // repair cost; well-formed files never reach it.
        Err(primary) => crate::repair::repair_load(data)
            .map_err(|_| primary.to_string())?,
    };
    if doc.trailer.has(b"Encrypt") || doc.was_encrypted() {
        return Err(format!(
            "{ENCRYPTED_PREFIX} this PDF is encrypted; load it with PdfDocument.load(bytes, {{ password }}) (use \"\" for owner-locked files)"
        ));
    }
    Ok(doc)
}
```

Note the error mapping: when repair also fails, surface the ORIGINAL lopdf error (`primary`), not the repair error — it's more diagnostic for well-formed-but-unsupported files.

- [ ] **Step 6: Run the full Rust suite**

Run: `cargo test -p better-pdf-core`
Expected: PASS (existing doc_io tests — encrypted rejection etc. — must be green; `garbage_still_fails` proves non-PDFs still error).

- [ ] **Step 7: Rebuild wasm and run the e2e evidence**

Run: `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts 2>&1 | tail -5`
Expected: the 9 previously-failing "loading tricky / malformed PDFs" tests and the `just_metadata.pdf` metadata test now PASS (fail count drops from 15 to 5; all remaining failures are in the `fancy_fields.pdf` describe block).

- [ ] **Step 8: Benchmark guard**

Run: `bun run bench`
Expected: no regression vs. `git stash`-baseline numbers (repair path is error-path-only). Record both numbers in the commit message body.

- [ ] **Step 9: Commit**

```bash
git add crates/core/src/repair.rs crates/core/src/doc_io.rs crates/core/src/lib.rs tests/fixtures/pdf-lib tests/pdf-lib-ported.test.ts
git commit -m "fix(core): recovery loader for PDFs with broken xref/trailer/header offsets

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: root-catalog recovery for `invalid_root_ref.pdf`

`invalid_root_ref.pdf` sometimes parses but its trailer `/Root` points at a non-catalog object, so `getPageCount()` silently returns 0. After Task 1 the repair loader may already catch it (lopdf currently fails its xref, triggering repair, which rescans /Root) — verify first; add the guard only if still broken.

**Files:**
- Modify: `crates/core/src/doc_io.rs` (`load_pdf`, after the Task 1 change)

**Interfaces:**
- Consumes: `repair_load` from Task 1.
- Produces: no new API; `load_pdf` additionally validates that `/Root` resolves to a dictionary with a `/Pages` key and re-runs repair when it doesn't.

- [ ] **Step 1: Check whether Task 1 already fixed it**

Run: `bun test tests/pdf-lib-ported.test.ts -t invalid_root_ref`
If PASS: skip to Step 5 (still add the Rust regression test below — it must pass as-is).

- [ ] **Step 2: Write the failing/regression Rust test**

Append to `doc_io.rs` tests:

```rust
#[test]
fn recovers_invalid_root_ref() {
    const INVALID_ROOT: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/invalid_root_ref.pdf");
    let doc = load_pdf(INVALID_ROOT).unwrap();
    assert!(!doc.get_pages().is_empty(), "must recover the real catalog");
}
```

Run: `cargo test -p better-pdf-core recovers_invalid_root_ref` — expected FAIL (0 pages) if Step 1 showed the bug persists.

- [ ] **Step 3: Implement the validity check in `load_pdf`**

Extend the Task 1 `load_pdf` body — after obtaining `doc` from the strict parse but before the encryption check:

```rust
    // A parse can "succeed" with a /Root pointing at a non-catalog object
    // (pdf-lib's invalid_root_ref.pdf). Treat that as a failed parse too.
    let doc = if root_is_valid(&doc) {
        doc
    } else {
        crate::repair::repair_load(data)
            .map_err(|_| "invalid /Root reference and repair failed".to_string())?
    };
```

with this helper in `doc_io.rs`:

```rust
/// True when the trailer /Root resolves to a dictionary that has /Pages.
fn root_is_valid(doc: &Document) -> bool {
    doc.trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_dictionary(id).ok())
        .map(|d| d.has(b"Pages"))
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p better-pdf-core` then `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts -t invalid_root_ref`
Expected: PASS, and no other test regresses (`cargo test` fully green — `root_is_valid` must not reject any existing fixture; if it does, the helper is wrong, not the fixture).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/doc_io.rs
git commit -m "fix(core): re-run recovery when trailer /Root is not a catalog

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: decode UTF-16BE field names (bug 1)

Field names with a `FE FF` BOM (`fancy_fields.pdf`) currently surface as raw bytes because `forms.rs::name_part` uses `String::from_utf8_lossy`. lopdf's `decode_text_string` already implements BOM-aware decoding (it's used for values at `forms.rs:450`). Because `fill.rs::find_field` matches via the same `fully_qualified_name`, fixing `name_part` fixes both read and fill/flatten sides at once.

**Files:**
- Modify: `crates/core/src/forms.rs:374-379` (`name_part`), `crates/core/src/forms.rs:344-347` (`inherited_str` stays byte-based for /DA — do not touch)

**Interfaces:**
- Consumes: `lopdf::decode_text_string` (already imported at `forms.rs:2`).
- Produces: `name_part` (same signature `pub(crate) fn name_part(d: &Dictionary) -> Option<String>`) now returns decoded Unicode.

- [ ] **Step 1: Write the failing Rust test**

Append to the `tests` module in `forms.rs`:

```rust
const FANCY: &[u8] = include_bytes!("../../../tests/fixtures/pdf-lib/fancy_fields.pdf");

#[test]
fn decodes_utf16_field_names() {
    let f = fields(FANCY);
    let names: Vec<&str> = f
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"First Name 🚀"), "names were {names:?}");
    assert!(names.contains(&"Historical Figures 🐺"));
    assert!(names.contains(&"Choose A Gundam 🤖"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p better-pdf-core decodes_utf16_field_names`
Expected: FAIL — names contain `þÿ\u{0}F\u{0}i…` garbage.

- [ ] **Step 3: Implement**

Replace `name_part` (forms.rs:374-379):

```rust
pub(crate) fn name_part(d: &Dictionary) -> Option<String> {
    // /T is a PDF text string: may be UTF-16BE with a FE FF BOM.
    d.get(b"T").ok().and_then(|o| decode_text_string(o).ok())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p better-pdf-core`
Expected: `decodes_utf16_field_names` PASSES; every existing test (viajero/ficha names are plain ASCII, unaffected) stays green.

- [ ] **Step 5: e2e check**

Run: `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts -t "enumerates all 15 fields"`
Expected: PASS if bug 2's indirect-`/V` issue doesn't intersect (field *names* only need this fix). The "fills text, toggles checkboxes" test may still fail on the radio assertion — that's Task 5.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/forms.rs
git commit -m "fix(core): decode UTF-16BE (BOM) field names via decode_text_string

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: resolve indirect references in `/V`, `/DV`, `/Opt` (bug 2)

`fancy_fields.pdf` stores the dropdown's value as `/V 1404 0 R` (an indirect reference to the string `(Dynames)`) and its options as `/Opt 514 0 R`. `forms.rs::field_value` and the `/Opt` read in `describe_field` (and `fill.rs::has_opt`/`dropdown_index`) never dereference, so values read as `null` and option lists come back empty.

**Files:**
- Modify: `crates/core/src/forms.rs:165-166, 178-182, 433-453, 470-475` (`describe_field`, `field_value`, `value_to_string`, `opt_export`)
- Modify: `crates/core/src/fill.rs:933-946` (`has_opt`, `dropdown_index`) and `fill.rs:776` (current `/V` read)

**Interfaces:**
- Produces: `pub(crate) fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object` in `forms.rs` — dereferences `Object::Reference` one level (recursively, capped), identity otherwise. Used by all four call sites.
- Changed signatures (all `pub(crate)`/private, callers updated in this task): `field_value(doc: &Document, d: &Dictionary, key: &[u8])`, `value_to_string(doc: &Document, o: &Object)`, `opt_export(doc: &Document, o: &Object)`, `has_opt(doc: &Document, dict: &Dictionary)`, `dropdown_index(doc: &Document, dict: &Dictionary, value: &str)`.

- [ ] **Step 1: Write the failing Rust test**

Append to `forms.rs` tests:

```rust
#[test]
fn resolves_indirect_value_and_options() {
    let f = fields(FANCY);
    let dropdown = f
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Choose A Gundam 🤖")
        .expect("dropdown present (requires Task 3)");
    assert_eq!(dropdown["value"], "Dynames");
    let opts = dropdown["options"].as_array().unwrap();
    assert!(!opts.is_empty(), "indirect /Opt must be dereferenced");
    assert!(opts.iter().any(|o| o == "Dynames"), "opts were {opts:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p better-pdf-core resolves_indirect_value_and_options`
Expected: FAIL — value is `null`, options empty.

- [ ] **Step 3: Implement**

Add to `forms.rs` (near `as_dict`):

```rust
/// Follow Object::Reference chains (max 32 hops) to the target object.
/// Non-references are returned as-is; a dangling reference returns itself.
pub(crate) fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    let mut cur = o;
    for _ in 0..32 {
        match cur {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(next) => cur = next,
                Err(_) => return cur,
            },
            _ => return cur,
        }
    }
    cur
}
```

Rework the value/option readers to resolve at every level:

```rust
fn field_value(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key).ok().map(|o| resolve(doc, o)).and_then(|o| match o {
        Object::Array(a) => {
            let parts: Vec<String> =
                a.iter().filter_map(|e| value_to_string(doc, e)).collect();
            if parts.is_empty() { None } else { Some(parts.join(", ")) }
        }
        other => value_to_string(doc, other),
    })
}

fn value_to_string(doc: &Document, o: &Object) -> Option<String> {
    match resolve(doc, o) {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        s @ Object::String(_, _) => decode_text_string(s).ok(),
        _ => None,
    }
}

pub(crate) fn opt_export(doc: &Document, o: &Object) -> String {
    match resolve(doc, o) {
        Object::Array(a) => a.first().and_then(|e| value_to_string(doc, e)).unwrap_or_default(),
        other => value_to_string(doc, other).unwrap_or_default(),
    }
}
```

Update the call sites in `describe_field` (forms.rs:165-166, 178-182, 256):

```rust
    let value = field_value(doc, d, b"V");
    let default_value = field_value(doc, d, b"DV");
    // …
    let options = d
        .get(b"Opt")
        .ok()
        .map(|o| resolve(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().map(|e| opt_export(doc, e)).collect())
        .unwrap_or_default();
    // … tooltip:
    tooltip: d.get(b"TU").ok().and_then(|o| value_to_string(doc, o)),
```

(`describe_field`'s `Opt` read currently chains `.and_then(|o| o.as_array())` on the `lopdf::Result` — the rewrite above goes through `.ok()` first; keep whichever chaining compiles cleanly with lopdf 0.41's `Dictionary::get` return type.)

In `fill.rs`, thread `doc` into the two `/Opt` helpers and the `/V` read (fill.rs:776), resolving the same way:

```rust
fn has_opt(doc: &Document, dict: &Dictionary) -> bool {
    dict.get(b"Opt")
        .ok()
        .map(|o| forms::resolve(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

fn dropdown_index(doc: &Document, dict: &Dictionary, value: &str) -> Option<i64> {
    let arr = forms::resolve(doc, dict.get(b"Opt").ok()?).as_array().ok()?;
    arr.iter()
        .position(|o| forms::opt_export(doc, o) == value)
        .map(|i| i as i64)
}
```

and at fill.rs:776:

```rust
    dict.get(b"V")
        .ok()
        .map(|o| forms::resolve(doc, o))
        .and_then(|o| decode_text_string(o).ok())
```

(this function must gain a `doc: &Document` parameter if it doesn't have one — update its callers mechanically; the compiler will list them.)

Also update `fill.rs:506` and `fill.rs:597,511` `/Opt` reads to go through `forms::resolve` the same way — search `grep -n 'b"Opt"' crates/core/src/fill.rs` and fix every hit.

- [ ] **Step 4: Run tests**

Run: `cargo test -p better-pdf-core`
Expected: new test PASSES, all existing fill/forms tests stay green (direct objects resolve to themselves, so behavior is unchanged for well-formed fixtures).

- [ ] **Step 5: e2e check**

Run: `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts -t "dropdown and listbox"`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/forms.rs crates/core/src/fill.rs
git commit -m "fix(core): dereference indirect /V, /DV and /Opt in field reads and fills

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: map radio-group values through `/Opt` (bug 3)

`fancy_fields.pdf`'s radio group stores `/V /0` with `/Opt [(Marcus Aurelius 🏛️) …]` — the on-state is an index. pdf-lib (and Acrobat semantics) report the `/Opt` label. Two sides: **read** (`describe_field` must map value `"0"` → `options[0]`) and **write** (`select("Alexander Hamilton 🇺🇸")` must map the label back to its index on-state).

**Files:**
- Modify: `crates/core/src/forms.rs` (`describe_field`, after the `options` binding)
- Modify: `crates/core/src/fill.rs` (the radio/checkbox state-selection path that validates via `widget_has_state` around fill.rs:900-931)

**Interfaces:**
- Consumes: `options: Vec<String>` and `value: Option<String>` already computed in `describe_field` (Task 4 versions); `has_opt(doc, dict)` from Task 4.
- Produces: read-side mapping inline in `describe_field`; write-side helper `fn opt_index_state(doc: &Document, dict: &Dictionary, label: &str) -> Option<String>` in `fill.rs`.

- [ ] **Step 1: Write the failing Rust tests**

`forms.rs` tests:

```rust
#[test]
fn radio_value_maps_through_opt() {
    let f = fields(FANCY);
    let radio = f
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Historical Figures 🐺")
        .unwrap();
    assert_eq!(radio["type"], "radio");
    assert_eq!(radio["value"], "Marcus Aurelius 🏛️");
}
```

`fill.rs` tests (mirror the style of the existing fill tests around fill.rs:1700):

```rust
#[test]
fn radio_select_accepts_opt_label() {
    const FANCY: &[u8] = include_bytes!("../../../tests/fixtures/pdf-lib/fancy_fields.pdf");
    let ops = r#"[{"name":"Historical Figures 🐺","value":"Alexander Hamilton 🇺🇸"}]"#;
    let out = fill_fields_json(FANCY, ops, false).unwrap();
    let fields: serde_json::Value =
        serde_json::from_str(&crate::forms::read_fields_json(&out).unwrap()).unwrap();
    let radio = fields
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Historical Figures 🐺")
        .unwrap();
    assert_eq!(radio["value"], "Alexander Hamilton 🇺🇸");
}
```

(Adjust the fill-entry JSON shape to match the existing fill tests' wire format — copy from a neighboring radio test in fill.rs's test module, e.g. the one asserting `Object::Name(n) if n == b"Titular"` at fill.rs:1708.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p better-pdf-core radio_value_maps_through_opt radio_select_accepts_opt_label`
Expected: FAIL — read side returns `"0"`; write side errors with "no widget with state" (or the module's equivalent message at fill.rs:920).

- [ ] **Step 3: Implement read side**

In `describe_field`, after `value`/`options` are computed (post-Task 4), insert:

```rust
    // Radio /Opt semantics (PDF 32000-1 §12.7.4.2.3): when /Opt is present the
    // widget on-states are indices into it; surface the /Opt label instead.
    let map_opt = |v: Option<String>| -> Option<String> {
        let v = v?;
        if field_type == "radio" && !options.is_empty()
            && let Ok(i) = v.parse::<usize>()
            && let Some(label) = options.get(i)
        {
            return Some(label.clone());
        }
        Some(v)
    };
    let value = map_opt(value);
    let default_value = map_opt(default_value);
```

(Place it after the `options` binding; reorder the existing `let value/default_value` lines above `options` accordingly — the compiler enforces the order.)

- [ ] **Step 4: Implement write side**

In `fill.rs`, in the radio selection path where the requested state is validated against widget on-states (the function that emits the "no widget with state" error ending at fill.rs:923), first try the literal state, then fall back to the `/Opt` index:

```rust
/// When a radio group carries /Opt, its on-states are indices; translate an
/// /Opt label to its index state ("Marcus Aurelius 🏛️" -> "0").
fn opt_index_state(doc: &Document, dict: &Dictionary, label: &str) -> Option<String> {
    let arr = forms::resolve(doc, dict.get(b"Opt").ok()?).as_array().ok()?;
    arr.iter()
        .position(|o| forms::opt_export(doc, o) == label)
        .map(|i| i.to_string())
}
```

and at the validation site, before erroring:

```rust
    let effective = if widgets_have_state(/* existing check */) {
        value.to_string()
    } else if let Some(idx_state) = opt_index_state(doc, field_dict, value) {
        idx_state
    } else {
        return Err(/* existing "no widget with state" error */);
    };
```

(Integrate with the module's actual control flow — the exact function is the one that builds the error string at fill.rs:915-922; use `effective` for both `/V` and widget `/AS` from there on.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p better-pdf-core`
Expected: both new tests PASS; existing radio tests (ficha's `Titular` radio has no `/Opt`, so the literal-state fast path keeps them identical) stay green.

- [ ] **Step 6: e2e check**

Run: `bun run build:wasm && bun test tests/pdf-lib-ported.test.ts`
Expected: **0 failures** across all 65 tests. If "flatten() removes fields" still fails, the flatten path resolves names through the same `fully_qualified_name` — debug there before touching anything else.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/forms.rs crates/core/src/fill.rs
git commit -m "fix(core): radio groups with /Opt read and select by option label

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: full-suite verification, changelog, version bump

**Files:**
- Modify: `CHANGELOG.md`, `package.json` (version), `crates/core/Cargo.toml` (version)

**Interfaces:** none — release chores.

- [ ] **Step 1: Full verification**

Run each and confirm green:

```bash
cargo test -p better-pdf-core
cargo clippy -p better-pdf-core -- -D warnings
bun run build:wasm && bun run build:js
bun test
bun run typecheck
bun run bench
```

Expected: all pass; bench within noise of the pre-change baseline (repair code is error-path only). `qpdf-validate.test.ts` in the full suite double-checks output validity.

- [ ] **Step 2: Changelog + version**

Bump to **1.13.0** (parser recovery is new capability; the rest are fixes) in `package.json` and `crates/core/Cargo.toml`. Add to `CHANGELOG.md` following its existing format:

```markdown
## 1.13.0

- **Recovery loader**: PDFs with broken or missing xref tables/trailers, junk
  before the `%PDF` header, invalid `/Root` references, or missing
  `endstream`/`endobj` keywords are now repaired on load instead of failing
  (ported pdf-lib robustness corpus).
- **Fix**: field names stored as UTF-16BE text strings (FE FF BOM) are decoded
  correctly (affects lookup, fill, and flatten by name).
- **Fix**: indirect references in `/V`, `/DV`, and `/Opt` are dereferenced when
  reading and filling fields.
- **Fix**: radio groups with `/Opt` report and accept the option label instead
  of the raw index on-state.
- Test suite: added `tests/pdf-lib-ported.test.ts` (65 behavioral tests ported
  from pdf-lib) plus its fixture corpus under `tests/fixtures/pdf-lib/`.
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md package.json crates/core/Cargo.toml
git commit -m "chore: release 1.13.0 — parser recovery + pdf-lib ported-test fixes

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Fixture-size note (decide before merging)

`tests/fixtures/pdf-lib/` adds ~43 MB, dominated by `bixby_guide.pdf` (23 MB) and `with_large_page_count.pdf` (8 MB). If repo size matters, delete those two files and their two test cases in `tests/pdf-lib-ported.test.ts` (the `bixby_guide.pdf` entry in the `cases` array and the `with_large_page_count.pdf` test) — the remaining corpus still covers every bug. This is the user's call; default is keep.
