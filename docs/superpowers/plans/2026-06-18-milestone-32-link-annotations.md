# Milestone M32: Link Annotations (URI + internal GoTo) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Add clickable link annotations — external URIs and internal page jumps — to pages. `page.drawLink({x,y,width,height, url})` and `page.drawLink({x,y,width,height, goToPage})`, on loaded and created PDFs.

**Architecture:** A `link` op appends a `/Annot /Subtype /Link` dictionary to the target page's `/Annots` array. URI links use `/A << /S /URI /URI (...) >>`; internal links use `/Dest [<targetPageRef> /XYZ null null null]`. On created PDFs (create.rs) this reuses the existing per-page annotation mechanism (the form-widget code already builds `/Annots`). On loaded PDFs (draw.rs) the page is already cloned for draw ops; the link annot object is added and its reference appended to the page's `/Annots` (handling array-or-reference-or-absent). A default `/Border [0 0 0]` suppresses the visible rectangle.

**Tech Stack:** Rust 2024, lopdf 0.41; TS ESM; Bun + cargo.

## Global Constraints

- Exactly one of `uri` / `goToPage` per link op; validate (reject neither/both).
- `Rect [x0 y0 x1 y1]` with `x1>x0`, `y1>y0`, all finite. `goToPage` must be a valid page index in the OUTPUT document.
- `/Border [0 0 0]` by default (no visible box).
- Internal GoTo target = the target page's object reference + `/XYZ null null null` (jump to page, preserve view).
- Both engines; existing draw/create/forms paths unchanged. Validate before mutation.
- Loaded: incremental; the link annot is a new object in `inc.new_document`; appended to the cloned page's `/Annots` (create the array if absent; if `/Annots` is an indirect reference, clone+append like the Resources handling).
- Every task green: cargo + bun + typecheck. No root Cargo.toml. Rebuild wasm before bun. pkg-web gitignored. Tests in `tests/`. Branch `m32-link-annotations`; not on master.

## File Structure

- Modify: `crates/core/src/draw.rs` — `DrawOp::Link`; validate; append annot to cloned page `/Annots`.
- Modify: `crates/core/src/create.rs` — `CreateOp::Link`; validate; append annot via the page_annots mechanism.
- (Optional) small shared helper for building the Link annot dict, e.g. in `draw.rs` `pub(crate) fn link_annot_dict(rect, target) -> Dictionary` reused by create.rs.
- Modify: `src/generate/draw-queue.ts` — `LinkOp` + `pushLink`; `src/generate/page.ts` — `drawLink`.
- Tests: draw.rs/create.rs `#[cfg(test)]`, `tests/link-annotations.test.ts`.

## Interfaces (cross-task contract)

- Wire op (both engines): `{"op":"link","page":i,"rect":[x0,y0,x1,y1],"uri":"https://..."}` OR `{"op":"link","page":i,"rect":[...],"goToPage":k}`. `uri`/`goToPage` both optional in the struct; validation enforces exactly one.
- `DrawOp::Link`/`CreateOp::Link` fields: `page: usize, rect: [f32;4], uri: Option<String>, #[serde(rename="goToPage")] go_to_page: Option<usize>`.
- TS: `page.drawLink(opts: {x:number; y:number; width:number; height:number; url?: string; goToPage?: number}): void`. DrawQueue `pushLink({page, rect, uri?, goToPage?})`.

---

### Task 1: Rust — link op in both engines

**Files:** `crates/core/src/draw.rs`, `crates/core/src/create.rs`.

- [ ] **Step 1: Write failing tests**

```rust
// draw.rs (loaded)
#[test]
fn appends_uri_link_annotation() {
    let out = apply_draw_ops_json(FICHA,
        r#"[{"op":"link","page":0,"rect":[50,50,200,80],"uri":"https://example.com"}]"#, &[], &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let page = doc.get_dictionary(pid).unwrap();
    let annots = resolve_annots(&doc, page); // helper: returns Vec<&Dictionary> of annot dicts
    let link = annots.iter().find(|a| a.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Link"))
        .expect("a /Link annot");
    assert_eq!(link.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    let a = link.get(b"A").unwrap().as_dict().unwrap();
    assert_eq!(a.get(b"S").unwrap().as_name().unwrap(), b"URI");
    let uri = a.get(b"URI").unwrap().as_str().unwrap();
    assert_eq!(uri, b"https://example.com");
}

#[test]
fn appends_goto_link_with_dest() {
    let out = apply_draw_ops_json(FICHA,
        r#"[{"op":"link","page":0,"rect":[10,10,100,40],"goToPage":0}]"#, &[], &[], "[]").unwrap();
    let doc = Document::load_mem(&out).unwrap();
    let (_, pid) = doc.get_pages().into_iter().next().unwrap();
    let annots = resolve_annots(&doc, doc.get_dictionary(pid).unwrap());
    let link = annots.iter().find(|a| a.has(b"Dest")).expect("a link with /Dest");
    assert!(link.get(b"Dest").unwrap().as_array().is_ok());
}

#[test]
fn link_rejects_both_uri_and_goto() {
    let r = apply_draw_ops_json(FICHA,
        r#"[{"op":"link","page":0,"rect":[0,0,10,10],"uri":"x","goToPage":0}]"#, &[], &[], "[]");
    assert!(r.is_err());
}

#[test]
fn link_rejects_neither() {
    let r = apply_draw_ops_json(FICHA, r#"[{"op":"link","page":0,"rect":[0,0,10,10]}]"#, &[], &[], "[]");
    assert!(r.is_err());
}
```
(Write a `resolve_annots(&Document, &Dictionary) -> Vec<&Dictionary>` test helper that resolves `/Annots` whether it's an inline array or a reference, and dereferences each entry.) Mirror a created-doc test in create.rs: `{"op":"addPage",...},{"op":"link","page":0,"rect":[...],"uri":"..."}` → the created page's `/Annots` has a `/Link`.

- [ ] **Step 2: Run — expect FAIL**

- [ ] **Step 3: Implement**

- Add `DrawOp::Link` (draw.rs, `#[serde(rename="link")]` on the variant since the enum is lowercase) and `CreateOp::Link` (create.rs, camelCase auto). Fields per the contract.
- Shared helper `pub(crate) fn link_annot_dict(rect: [f32;4], uri: Option<&str>, dest_page: Option<ObjectId>) -> Dictionary` building:
  ```
  /Type /Annot /Subtype /Link
  /Rect [x0 y0 x1 y1] (Real)
  /Border [0 0 0]
  + if uri: /A << /S /URI /URI (uri) >>
  + if dest_page: /Dest [Reference(dest_page) /XYZ Null Null Null]
  ```
- Validation (both engines): `page < page_count`; exactly one of `uri`/`go_to_page` (else err); rect finite + x1>x0 + y1>y0; if `go_to_page`, it must be `< page_count`.
- draw.rs: resolve the target page id for `go_to_page` from the prev document's sorted pages (same way the code derives the current `page_id`). Build the annot dict (uri or dest=Reference(target_page_id)), add it to `inc.new_document` → annot_id. Append `Reference(annot_id)` to the cloned page's `/Annots`: read existing `/Annots` (inline array → push; indirect reference → clone the array object into new_document and push; absent → create new array). This mirrors the existing Resources/Contents indirect-handling pattern (`dict_mut`, `opt_clone_object_to_new_document`). A page touched ONLY by a link op must still clone + append (it does, since the link op puts the page in `page_ops`), and the empty-content guard (from M29) means no draw stream is appended.
- create.rs: target page id for goToPage = `page_ids[go_to_page]`. Build the annot, `doc.add_object` it, and push its id into the page's annots (reuse the existing `page_annots[page]` Vec that widget code appends to — the link annot id goes there and gets written to the page `/Annots` by the existing annot-append loop). If links can appear on pages with no form fields, ensure the page_annots mechanism still runs (it iterates all pages). Add a no-op arm for Link in the content-drawing match.

> VERIFY: lopdf `Object::Null` for the /Dest /XYZ nulls; `Object::string_literal` for the URI; how the existing create.rs `page_annots` loop writes `/Annots` (reuse it). The 4+ tests are the gate.

- [ ] **Step 4: Run — expect PASS, full suite**

- [ ] **Step 5: Commit**

```bash
git checkout -b m32-link-annotations
git add crates/core/src/draw.rs crates/core/src/create.rs
git commit -m "feat(links): URI + internal GoTo link annotations on loaded and created PDFs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: TypeScript — page.drawLink

**Files:** `src/generate/draw-queue.ts`, `src/generate/page.ts`.

- [ ] **Step 1: Rebuild wasm.**
- [ ] **Step 2: Failing test** (`tests/link-annotations.test.ts`): create a doc, addPage, `page.drawLink({x:50,y:50,width:150,height:30,url:"https://example.com"})`, save, reload → valid. A loaded-doc variant. A `goToPage` variant. (Assert round-trip validity + page count; structural /Annots assertion lives in the Rust tests. If easy, also assert via a second reload that it stays valid.)
- [ ] **Step 3: Implement**
  - `draw-queue.ts`: `LinkOp = {op:"link"; page:number; rect:[number,number,number,number]; uri?:string; goToPage?:number}`; `pushLink(op: LinkOp)` pushing onto drawOps (plain op, flows through buildDrawOps to both payloads, like setRotation).
  - `page.ts`: `drawLink(opts: {x:number;y:number;width:number;height:number;url?:string;goToPage?:number}): void` — validate finite + width>0 + height>0 (RangeError); validate exactly one of url/goToPage provided (throw a clear error if neither/both); compute `rect=[x, y, x+width, y+height]`; `this.drawQueue.pushLink({op:"link", page:this.index, rect, ...(url!==undefined?{uri:url}:{}), ...(goToPage!==undefined?{goToPage}:{})})`. Works both modes.
- [ ] **Step 4: Run focused + full + typecheck + cargo. Green.**
- [ ] **Step 5: Commit** (`feat(links): page.drawLink TS API`)

---

### Task 3: Docs + version 0.10.0

**Files:** generating.md, limitations.md, from-pdf-lib.md, SKILL.md, README.md, CHANGELOG.md, package.json, Cargo.toml.

- [ ] **Step 1: Docs** — "Links" section (URI + internal page jump examples). limitations.md: link annotations now SUPPORTED (URI + GoTo); note named destinations / link borders styling are minimal (border suppressed by default). from-pdf-lib.md: note this is more ergonomic than pdf-lib's low-level annotation API. SKILL.md + README.md.
- [ ] **Step 2: Version** 0.10.0 (package.json + Cargo.toml). CHANGELOG 0.10.0: "Link annotations: `page.drawLink()` for external URIs and internal page jumps, on loaded and created PDFs."
- [ ] **Step 3: TypeDoc regen if clean** (discard spurious Cargo.lock churn if any; the version bump IS a real lock change — keep that).
- [ ] **Step 4: Final verify (cargo + bun + typecheck) + commit** (`docs(links): document link annotations; release 0.10.0`).

---

## Self-Review

**Spec coverage:** URI link (T1), GoTo link (T1), both engines, validation (exactly-one, rect, page ranges), TS drawLink (T2), docs/version (T3).

**Risk callouts:** (1) loaded-page `/Annots` may be an indirect reference — handle clone+append (mirror Resources handling); (2) link-only page must clone + append without an empty draw stream (M29 empty-content guard already covers this); (3) GoTo target page ref: prev-doc sorted pages (loaded) / page_ids (created); (4) verify lopdf `Object::Null` + how create.rs writes `/Annots`.

**Type consistency:** `link` op fields `page/rect/uri/goToPage` identical across DrawOp/CreateOp/TS. `link_annot_dict(rect, uri, dest_page)` shared by both engines. TS computes `rect=[x,y,x+width,y+height]`.
