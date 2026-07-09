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

### 2. Filling a std-14 `/DA` font absent from `/DR` throws
- **Fixture:** `tests/fixtures/pypdf/issues/iss2670-f1040.pdf` (IRS Form 1040)
- **Symptom:** `getTextField(name).setText(...)` then `save()` throws
  `PdfCoreError: DA font 'Helvetica' not found in /DR for ...`.
- **Expected:** when the field's `/DA` names a standard-14 font (Helvetica, etc.)
  that is missing from the AcroForm `/DR`, add it to the generated appearance's
  `/Resources/Font` instead of failing (pypdf `test_no_resource_for_14_std_fonts`).
- **Impact:** blocks filling many real government forms (f1040 and similar).
- **Test:** `test.skip("iss2670: filling a std-14 /DA font absent from /DR")`.

### 3. Hierarchical dotted field names not resolved
- **Fixtures:** `issues/fields_with_dots.pdf`, `issues/iss2643-inheritance.pdf`
- **Symptom:** a parent field with terminal kids (e.g. `customer` → `name`) is
  reported as a single field named `customer` with `type: "unknown"`; the
  qualified child name `customer.name` is never exposed. Deeply-nested inherited
  names (`amt1.0`, `Text10.0.0.1.1...`) are likewise missing.
- **Expected:** resolve terminal descendants to `parent.child` qualified names
  (pypdf `get_fields`/`get_form_text_fields` with qualified=True).
- **Tests:** `test.skip("fields_with_dots: qualified child name customer.name is resolved")`;
  `iss2643` currently only resolves top-level names (`Text10`, `DSS#3pg3#0hgu7`).

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
