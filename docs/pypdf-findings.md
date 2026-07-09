# pypdf-ported test suite — follow-up findings

Surfaced while porting behavioral tests from [pypdf](https://github.com/py-pdf/pypdf)
into `tests/pypdf-ported.test.ts` (2026-07-09). Items to revisit.

## Bugs

### 1. AESV2 without `/Length` fails to decrypt
- **Fixture:** `tests/fixtures/pypdf/encryption/r4-aes-v2-no-key-length.pdf`
- **Symptom:** `PdfDocument.load(bytes, { password: "" })` throws `IncorrectPasswordError`.
- **Expected:** decrypts to a 1-page doc. When `/Encrypt` omits `/Length`, the key
  length should default to 128-bit by reading the crypt-filter (`/CF`) dict, not the
  spec's 40-bit default for the top-level dict.
- **Root cause:** decryption is fully delegated to **lopdf 0.41**
  (`crates/core/src/doc_io.rs`, `load_mem_with_options`), which defaults to 40-bit here.
- **Fix options:** (a) upstream a fix to lopdf; (b) pre-inject `/Length 128` into the
  `/Encrypt` dict before handing bytes to lopdf.
- **Test:** `tests/pypdf-ported.test.ts` — `test.skip("r4-aes-v2-no-key-length ... KNOWN LIMITATION")`.
  Unskip once fixed.

### 2. Filling a std-14 `/DA` font absent from `/DR` throws — FIXED
- **Fixture:** `tests/fixtures/pypdf/issues/iss2670-f1040.pdf` (IRS Form 1040)
- **Was:** `getTextField(name).setText(...)` then `save()` threw
  `PdfCoreError: DA font 'Helvetica' not found in /DR for ...`.
- **Fix:** `fill.rs` — when the DA font is absent from `/DR` but names a
  standard-14 text font (via `da_font_base`, accepting canonical names and the
  `Helv`/`TiRo`/… aliases), synthesize a Type1 font dict at apply time and put
  it in the appearance's `/Resources/Font` (`FontRef::Synth` +
  `resolve_font_ref`) instead of failing. Widths come from
  `standard_14_widths(base)`. Non-standard fonts still surface the
  "not found in /DR" error (Symbol/ZapfDingbats excluded — custom encodings).
- **Tests:** `tests/pypdf-ported.test.ts` "iss2670: filling a std-14 /DA font
  absent from /DR" (unskipped, green); Rust `fills_std14_da_font_absent_from_dr_by_synthesizing`,
  `unknown_da_font_absent_from_dr_still_errors`, `da_font_base_maps_names_and_aliases`.

### 3. Hierarchical dotted field names not resolved — FIXED
- **Fixtures:** `issues/fields_with_dots.pdf`, `issues/iss2643-inheritance.pdf`
- **Was:** a parent field with terminal kids (e.g. `customer` → `name`) was
  reported as a single field named `customer` with `type: "unknown"`; the
  qualified child name `customer.name` was never exposed. Deeply-nested names
  (`Text10.0.0.1.1...`) were likewise missing.
- **Fix:** `forms.rs` `collect_fields` now recurses via `walk_field`: a node
  whose `/Kids` include a field-kid (a kid carrying its own `/T`) is
  non-terminal and contributes only a name segment; its terminal descendants
  are emitted with fully-qualified `parent.child` names (built by the existing
  `fully_qualified_name` /Parent walk). Bounded by depth (`MAX_PARENT_DEPTH`)
  and a 100k output cap against cyclic `/Kids`. Fill already resolved qualified
  names via `find_field`. **Flatten** was updated in lockstep: `flatten.rs`
  `remove_fields` now prunes the `/Kids` tree recursively (`prune_field`) so
  flattening a nested child removes it and drops any emptied parent, instead of
  the old top-level-only `/Fields` filter (which would leave orphans).
- **Tests:** `tests/pypdf-ported.test.ts` — "fields_with_dots: qualified child
  name customer.name is resolved" (unskipped), "...is fillable", "...flatten
  clears the hierarchical fields", "iss2643: deep field tree resolves
  fully-qualified dotted names". Rust: `forms::expands_hierarchical_fields_into_qualified_names`,
  `flatten::flatten_removes_hierarchical_fields`.

### 4. Corrupted-xref recovery yields 0 pages
- **Fixture:** `tests/fixtures/pypdf/issues/iss2516.pdf`
- **Symptom:** loads without error but `getPageCount()` is 0 and `getPage(0)`
  throws `PageOutOfRangeError`. `repair.rs` byte-scan recovery does not kick in.
- **Expected:** recover the catalog and page tree (pypdf `test_corrupted_xref`
  asserts `root_object["/Type"] == "/Catalog"`). Note the truncated-xref sibling
  (`iss2575`) recovers fine, so the gap is specific to this corruption shape.
- **Test:** `test.skip("iss2516: corrupted xref recovers the page tree")`.

### 5. No orphaned-widget reattachment
- **Fixture:** `tests/fixtures/pypdf/issues/iss2453-ExampleForm.pdf`
- **Symptom:** better-pdf finds 8 form fields; the widget annotations not linked
  into `/AcroForm/Fields` are missed. pypdf's `reattach_fields()` relinks them,
  reaching 15.
- **Expected:** a repair/reattach step (or lenient field discovery via page
  `/Annots`) that surfaces orphaned widgets. No test written (no current API).

## Candidate API additions

### 6. `isEncrypted` predicate
pypdf exposes `reader.is_encrypted`. better-pdf only surfaces encryption by throwing on
use (lazy). A cheap `doc.isEncrypted` boolean would let callers branch without a
try/catch. See [[encrypted-pdf-load-api]] semantics.

### 7. User-vs-owner password-type reporting
pypdf's `decrypt()` returns `PasswordType.USER_PASSWORD` / `OWNER_PASSWORD`. better-pdf
just opens. Relevant for permission-aware flows.

## Ported (Tier 5, issue fixtures under `fixtures/pypdf/issues/`)

Now in `tests/pypdf-ported.test.ts`. Passing: iss3115 (button `/V` name object),
iss2611 (choice selection round-trip), iss2724 (flipped-BBox fill), tika-972486
(checkbox on-state), fields_with_dots (leaf field), iss2643 (deep tree top-level
names), iss2575 (truncated-xref rebuild). Skipped (track the gaps above): iss2670,
fields_with_dots qualified child, iss2516. Not written (no API): iss2453 reattach.

## Still not ported
- `test_matrix_entry_in_field_annots` (iss2731) — appearance `/Matrix` retained. Needs raw AP access to assert.
- Exact appearance-operator geometry (comb spacing, auto-size point sizes) — not portable byte-for-byte.

## Notes / non-bugs
- A file with an **empty owner password** opens with any string (spec-correct: empty
  owner = unprotected). Genuine wrong-password tests must use `r6-both-passwords.pdf`,
  not `r6-user-password.pdf`.
- `load()` is lazy: decryption/parse errors surface on first use (`getPageCount` /
  `getMetadata`), except files needing eager auth with no empty-password path, which
  throw during `load()`.
