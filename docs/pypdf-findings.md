# pypdf-ported test suite — follow-up findings

Surfaced while porting behavioral tests from [pypdf](https://github.com/py-pdf/pypdf)
into `tests/pypdf-ported.test.ts` (2026-07-09). Items to revisit.

## Bugs

### 1. AESV2 without `/Length` fails to decrypt — FIXED (workaround) + lopdf bug filed
- **Fixture:** `tests/fixtures/pypdf/encryption/r4-aes-v2-no-key-length.pdf`
- **Was:** `PdfDocument.load(bytes, { password: "" })` threw `IncorrectPasswordError`.
- **Root cause (confirmed in lopdf 0.41):** PDF §7.6.1 fixes the V4 file-encryption-key
  length at 128 bits, so a conforming V4 `/Encrypt` need not carry `/Length`. But
  `encryption/algorithms.rs` `PasswordAlgorithm::try_from` sets `length` only from the
  top-level `/Length` (never defaulting V4→128), and `compute_file_encryption_key_r4`
  then does `self.length.unwrap_or(40)` → a 40-bit key → wrong key → `InvalidPassword`.
  The one-line upstream fix is to default `length` to 128 for V=4 (256 for V=5) when
  the entry is absent. See `docs/lopdf-v4-length-issue.md` for the issue write-up.
- **Workaround (shipped):** `repair.rs` `inject_v4_length` — invoked **only after** a
  decrypt attempt fails: it injects `/Length 128` into a V4 `/Encrypt` dict that lacks
  a top-level `/Length` and rebuilds the file with a fresh xref, reusing the original
  `trailer` verbatim so `/ID` (hashed into the key) stays byte-exact. `decrypt_pdf`
  retries on the patched bytes. Because it runs post-failure, it can't affect
  well-formed files. Classic-trailer files only (xref-stream files decline → `None`).
- **Tests:** `tests/pypdf-ported.test.ts` "r4-aes-v2-no-key-length.pdf decrypts with
  empty password" (unskipped). Rust: `doc_io::decrypts_v4_aes128_missing_length_entry`,
  `doc_io::inject_v4_length_declines_non_matching_files`.

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

### 4. Corrupted-xref recovery yields 0 pages — FIXED
- **Fixture:** `tests/fixtures/pypdf/issues/iss2516.pdf`
- **Was:** loaded without error but `getPageCount()` was 0. The real corruption
  shape here isn't a broken xref (the strict parser *succeeds*): the catalog's
  `/Pages` reference names the wrong object (the Info dict, obj 6) while the true
  `/Type /Pages` node is obj 5. So `repair.rs` never ran and no page resolved.
- **Fix:** two parts. (a) `doc_io.rs` `root_is_valid` now rejects a catalog whose
  `/Pages` resolves to neither a `/Type /Pages` node nor any page, so recovery is
  triggered even when the strict parse "succeeds". (b) `repair.rs`
  `repair_page_tree` re-points a broken catalog `/Pages` at the real page-tree
  root (the `/Type /Pages` node that is not another node's kid) and fixes the
  kids' `/Parent`, when no page otherwise resolves.
- **Tests:** `tests/pypdf-ported.test.ts` "iss2516: corrupted /Pages reference
  recovers the page tree" (unskipped). Rust:
  `repair::repairs_catalog_pointing_pages_at_wrong_object`,
  `repair::load_pdf_recovers_corrupt_pages_reference`.

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
