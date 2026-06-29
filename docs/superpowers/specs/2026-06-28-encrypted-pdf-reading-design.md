# Reading & modifying encrypted PDFs (decrypt-at-load)

**Status:** Design approved — ready for implementation planning.
**Date:** 2026-06-28
**Scope:** Tier 2, item 2. Decrypt encrypted PDFs on load so they can be read and
modified; modifying produces a decrypted (unencrypted) output.

## Problem

Encrypted PDFs are currently rejected: `doc_io::load_pdf` returns an error with
the `ENCRYPTED:` prefix when the trailer carries `/Encrypt`, which the TS layer
maps to `EncryptedPdfError`. Many real-world PDFs (especially form documents) are
"encrypted" only in the sense of being owner-locked with an **empty user
password**, and are otherwise perfectly readable once decrypted. Users can't fill
or read these today.

pdf-lib — the library better-pdf positions against — does **not** decrypt at all:
it throws `EncryptedPDFError`, or with `{ ignoreEncryption: true }` skips the
check without decrypting (leaving ciphertext strings/streams, producing garbage
on save). better-pdf can do strictly better because lopdf ships real decryption.

## Key enabling fact

lopdf 0.41 (already a dependency) implements PDF decryption — RC4, AES-128, and
AES-256 — with a public API:

- `Document::decrypt(&mut self, password: &str) -> Result<(), lopdf::Error>` —
  authenticates `password` (user or owner) and decrypts every string/stream in
  place. Errors arrive as `Error::Decryption(DecryptionError)` (variants include
  `IncorrectPassword`, `Padding`, `UnsupportedEncryption`, `UnsupportedVersion`,
  `UnsupportedRevision`, and structural `Missing*`/`Invalid*`) or
  `Error::NotEncrypted`.
- `Document::is_encrypted()` — true when `/Encrypt` is present.
- `Document::encrypt(&mut self, state: &EncryptionState)` plus
  `EncryptionState::try_from(EncryptionVersion::{V2,V4,V5})` — used only to
  **generate test fixtures**.

The crypto crates (`aes`, `cbc`, `chacha20`, `cipher`) are already compiled in as
non-optional lopdf dependencies. This feature is **integration, not crypto
implementation**.

## Architecture decision (Approach A)

Decrypt **once at load** and cache the plaintext bytes. `PdfDocument` already
holds `this.bytes` and passes it to every WASM entry point
(`read_fields`, `fill_fields`, `flatten_fields`, `apply_draw_ops`,
`set_metadata`, `insert_pages`, the incremental `save()`, …). If `this.bytes` is
decrypted plaintext at load time, **every downstream operation and the
incremental save path work unchanged**, and "modify → decrypted output" falls out
for free (the cached bytes are already a valid unencrypted PDF). This avoids
threading a password through every entry point and avoids the
incremental-append-onto-an-encrypted-base problem entirely.

## API surface

```ts
PdfDocument.load(
  input: Uint8Array | ArrayBuffer,
  opts?: { password?: string },
): Promise<PdfDocument>
```

- **Decryption is opt-in via the `password` option** (decided after a benchmark
  review — see below). Bare `load(bytes)` (no `opts`, or `opts` without
  `password`) stays **lazy and unchanged** — it makes no WASM call, so the
  `load → mutate → save` benchmark is unaffected. Passing `password` (any string,
  including `""`) triggers the eager decrypt path.
- `password: ""` handles the common owner-locked / empty-user-password case.
- A supplied password is used for user **or** owner authentication.
- Passing `password` for a non-encrypted PDF is silently ignored (returns it
  unchanged).
- An encrypted file loaded **without** `password` is not decrypted; the first
  operation hits the existing `load_pdf` reject and throws `EncryptedPdfError`,
  whose message tells the caller to pass `{ password }`.

### Why opt-in (benchmark rationale)

Reliable encryption detection requires a full `Document::load_mem` parse (a cheap
byte-scan for `/Encrypt` is unreliable for xref-stream PDFs). Making bare `load`
eager would add that parse to **every** load — and the benchmark runs a full
`load → mutate → save` cycle per iteration on unencrypted fixtures, so it would
measurably regress (worst on large forms). Gating the eager path behind
`password` keeps the common path free.

## New WASM entry point

```rust
#[wasm_bindgen]
pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, JsError>
```

Behavior:

1. `Document::load_mem(data)`.
2. **Not encrypted** (`!doc.is_encrypted()`): return `data.to_vec()` **verbatim**
   — no re-serialization, so unencrypted PDFs are byte-identical to today and the
   existing "no-op save returns identical bytes" test keeps passing.
3. **Encrypted**: `doc.decrypt(password)`:
   - **Ok**: `doc.trailer.remove(b"Encrypt")` (required — otherwise lopdf's writer
     would attempt to re-encrypt on save), then `doc.save_to(&mut out)` and return
     `out` (clean plaintext bytes, no `/Encrypt`).
   - **Err**: classify and return a stable-prefixed error string:
     - password-related (`IncorrectPassword`, `Padding`, `MissingUserPassword`,
       `MissingOwnerPassword`) → **`PASSWORD:`** prefix.
     - everything else (`UnsupportedEncryption`/`Version`/`Revision`, structural
       `Invalid*`/`Missing*`) → **`ENCRYPTED:`** prefix (cannot open).

A small internal helper does the load/detect/decrypt/strip/save so the
`#[wasm_bindgen]` wrapper only maps errors to `JsError`.

## Load flow (TS)

`PdfDocument.load`:
- When `opts?.password === undefined`: construct the document with the raw bytes
  unchanged (lazy, exactly as today — **no WASM call**).
- When `opts.password` is defined (including `""`): call
  `wasm.decryptPdf(bytes, opts.password)` once and construct with the returned
  (plaintext) bytes as `this.bytes`.

No other TS or Rust code changes — all existing operations consume `this.bytes`.
When the password path is taken, `load()` is the fail-fast point for
password/encryption errors. The same change applies to both the Node entry
(`src/index.ts`) and the browser entry (`src/index.browser.ts`), whose `load`
implementations are separate.

## Error taxonomy

- **`EncryptedPdfError`** (existing): now covers two cases — (a) an encrypted file
  loaded **without** a `password` (the existing `load_pdf` reject path), and (b) an
  algorithm/revision lopdf can't open (`ENCRYPTED:` from `decrypt_pdf`). Message
  updated to tell the caller to pass `PdfDocument.load(bytes, { password })`.
- **`IncorrectPasswordError extends PdfError`** (new): wrong or missing password —
  mapped from the new `PASSWORD:` prefix. Message guides the caller to pass / fix
  `opts.password`.
- The `load_pdf` reject in `doc_io.rs` **stays** as defense-in-depth (still emits
  `ENCRYPTED:`) for any raw-bytes entry point that bypasses `load` — e.g.
  `merge`/`assemble` with encrypted source bytes, which remain unsupported and
  documented.

## Output semantics

Modifying (fill / flatten / draw / metadata / page-ops) an encrypted PDF produces
a **decrypted** output: the cached bytes are plaintext, `save()` is unchanged, and
the result reloads with no password and carries no `/Encrypt`. This is the
agreed "decrypted output" scope.

## Testing

**Fixtures** — add ignored `emit_encrypted_*` tests in the Rust suite (mirroring
the existing `emit_*_fixture` ignored-test pattern) that load the plain FICHA
fixture, encrypt it with lopdf's `Document::encrypt` via
`EncryptionState::try_from(EncryptionVersion::…)`, and write variants to
`tests/fixtures/generated/`:

- `ficha-rc4.pdf` — `EncryptionVersion::V2` (RC4, 128-bit), empty user password.
- `ficha-aes128.pdf` — `EncryptionVersion::V4` (AES-128 crypt filter), empty user
  password.
- `ficha-aes256.pdf` — `EncryptionVersion::V5` (AES-256), empty user password.
- `ficha-rc4-pw.pdf` — V2 with user password `"secret"`.

(V5 requires an explicit `file_encryption_key`; the generator supplies a fixed
test key. If V5 construction proves disproportionately fiddly, AES-256 may be
covered by a Rust-only encrypt-then-decrypt round-trip test instead of a
committed fixture — RC4 + AES-128 fixtures are the required minimum.)

**Rust unit tests** (`decrypt_pdf`):
- Unencrypted input → returns input bytes **unchanged** (byte-identical).
- RC4 / AES-128 / AES-256 empty-password fixtures → `Ok`; output has **no**
  `/Encrypt`; the document re-parses and its field set matches the plain FICHA.
- `"secret"`-password fixture: correct password → `Ok`; wrong password and empty
  password → `Err` with the `PASSWORD:` prefix.

**TS integration tests**:
- `load(encryptedBytes)` (empty password) → `getForm().getFields()` returns the
  expected fields.
- `load(encryptedBytes, { password: "secret" })` → reads.
- `load(encryptedBytes, { password: "wrong" })` → throws `IncorrectPasswordError`.
- Fill a field on an encrypted form → `save()` → reload the output **without** a
  password → the value is present and the output has no `/Encrypt`.

## Files touched (anticipated)

- `crates/core/src/doc_io.rs` — add the decrypt helper + classification; keep the
  existing reject net.
- `crates/core/src/lib.rs` — export `decrypt_pdf`.
- Fixture generators — alongside the existing `emit_*` ignored tests (likely in
  `forms.rs` or a dedicated module).
- `src/core/wasm.ts` / `wasm-browser.ts` — surface `decryptPdf` on the wasm
  binding interface.
- `src/index.ts` — `PdfDocument.load` gains the `opts` parameter and the decrypt
  call.
- `src/core/errors.ts` — add `IncorrectPasswordError`; update `EncryptedPdfError`
  message and the prefix→error mapping.
- Docs: `guides/filling-forms.md` or a short section, `reference/limitations.md`
  (the encrypted-PDF bullet), `README.md`, migration-from-pdf-lib note, CHANGELOG,
  version bump.

## Non-goals (YAGNI)

- Re-encrypting the output (output is always decrypted).
- Encrypting documents created by the builder (no encrypt API exposed).
- Password support on `merge` / `assemble` / other raw-bytes static helpers — they
  keep rejecting encrypted input; a possible later follow-up.
- Enforcing `/P` permission bits — we decrypt regardless; permission flags are
  advisory and not enforced by a programmatic library.
- Detecting/handling the distinction between user vs owner password beyond what
  lopdf's `decrypt` already does.
