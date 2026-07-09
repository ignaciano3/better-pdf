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

## Candidate API additions

### 2. `isEncrypted` predicate
pypdf exposes `reader.is_encrypted`. better-pdf only surfaces encryption by throwing on
use (lazy). A cheap `doc.isEncrypted` boolean would let callers branch without a
try/catch. See [[encrypted-pdf-load-api]] semantics.

### 3. User-vs-owner password-type reporting
pypdf's `decrypt()` returns `PasswordType.USER_PASSWORD` / `OWNER_PASSWORD`. better-pdf
just opens. Relevant for permission-aware flows.

## Not ported yet (need GitHub-issue fixture downloads)

From the pypdf survey, higher-value tests still to port once fixtures are fetched:
- `test_reattach_fields` (iss2453) — re-link orphaned widget annots to `/AcroForm/Fields`.
- `test_field_box_upside_down` (iss2724) — appearance BBox normalized positive for flipped widgets.
- `test_truncated_xref` (iss2575) / `test_corrupted_xref` (iss2516) — real-world xref rebuild.
- `test_no_resource_for_14_std_fonts` (iss2670) — add `/Helvetica` to appearance `/Resources/Font`.
- `test_i_in_choice_fields` (iss2611) — choice `/I` selected-index cleared on value set.

## Notes / non-bugs
- A file with an **empty owner password** opens with any string (spec-correct: empty
  owner = unprotected). Genuine wrong-password tests must use `r6-both-passwords.pdf`,
  not `r6-user-password.pdf`.
- `load()` is lazy: decryption/parse errors surface on first use (`getPageCount` /
  `getMetadata`), except files needing eager auth with no empty-password path, which
  throw during `load()`.
