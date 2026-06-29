# Encrypted PDF Reading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decrypt encrypted PDFs on load (RC4 / AES-128 / AES-256, optional password, empty default) so they can be read and modified, with modification producing a decrypted output.

**Architecture:** A new `decrypt_pdf(data, password)` WASM entry point. Decryption is **opt-in**: bare `PdfDocument.load(bytes)` stays lazy/unchanged; `PdfDocument.load(bytes, { password })` (password defined, even `""`) calls `decrypt_pdf`. Unencrypted input passes through verbatim; encrypted input is decrypted with lopdf, has `/Encrypt` stripped, and is re-serialized to plaintext. The decrypted bytes become `this.bytes`, so every existing operation and the incremental save path are untouched. Gating on `password` keeps the unencrypted benchmark hot path free of an extra parse.

**Tech Stack:** Rust (lopdf 0.41, which ships the RC4/AES decryption), WASM, TypeScript, Bun test runner, `cargo test`.

## Global Constraints

- Supported algorithms come entirely from lopdf: RC4, AES-128, AES-256. No crypto is implemented here.
- Decryption is **opt-in via `password`**: bare `load(bytes)` is lazy/unchanged (no WASM call, no benchmark regression); `load(bytes, { password })` (any string, incl. `""`) decrypts. `password: ""` handles owner-locked files. An encrypted file loaded without `password` throws `EncryptedPdfError` on first use (message nudges to pass one).
- Unencrypted input must pass through `decrypt_pdf` **byte-identical** (return `data.to_vec()`, no re-serialization) so the existing "no-op save returns identical bytes" test still passes.
- Decrypting strips `/Encrypt` (`doc.trailer.remove(b"Encrypt")`) before `save_to`, or lopdf's writer would re-encrypt.
- Error prefixes (stable, matched by the TS layer): password failures → `PASSWORD:`; unsupported/unreadable encryption → `ENCRYPTED:` (the existing prefix).
- The existing `load_pdf` reject in `doc_io.rs` stays as defense-in-depth for raw-bytes entry points (merge/assemble), still emitting `ENCRYPTED:`.
- Modifying an encrypted PDF yields a decrypted output (no `/Encrypt`); reloads without a password.
- WASM must be rebuilt (`bun run build:wasm`) after Rust changes before TS tests run against it.
- Spec: `docs/superpowers/specs/2026-06-28-encrypted-pdf-reading-design.md`.

---

### Task 1: Rust — encrypted fixtures + `decrypt_pdf` core + WASM export

**Files:**
- Modify: `crates/core/src/doc_io.rs` (add `decrypt_pdf`, `PASSWORD_PREFIX`, fixture-generator ignored test, decrypt unit tests)
- Modify: `crates/core/src/lib.rs` (export `decrypt_pdf` as a `#[wasm_bindgen]` fn)
- Create (generated, committed): `tests/fixtures/generated/ficha-rc4.pdf`, `ficha-aes128.pdf`, `ficha-aes256.pdf`, `ficha-rc4-pw.pdf`

**Interfaces:**
- Consumes: lopdf `Document::{load_mem, decrypt, encrypt, save_to}`, `Document::trailer`, `lopdf::{EncryptionVersion, EncryptionState, Permissions}`, `lopdf::encryption::{DecryptionError, crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter}}`, `crate::forms::read_fields_json`.
- Produces: `pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, String>` in `doc_io`; `pub const PASSWORD_PREFIX: &str = "PASSWORD:"`; the `#[wasm_bindgen] pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, JsError>` in `lib.rs`.

- [ ] **Step 1: Add the fixture generator (ignored test)**

In `crates/core/src/doc_io.rs`, inside the `#[cfg(test)] mod tests` block, add a `FICHA` include and an ignored generator. (The plain fixture path matches the one used elsewhere in the crate.)

```rust
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    /// Encrypt FICHA with the standard security handler and return the bytes.
    /// `user_pw` is the user password (empty for the common owner-locked case).
    fn encrypt_rc4(user_pw: &str) -> Vec<u8> {
        use lopdf::{EncryptionState, EncryptionVersion, Permissions};
        let mut doc = Document::load_mem(FICHA).unwrap();
        let version = EncryptionVersion::V2 {
            document: &doc,
            owner_password: "owner",
            user_password: user_pw,
            key_length: 128,
            permissions: Permissions::all(),
        };
        let state = EncryptionState::try_from(version).unwrap();
        doc.encrypt(&state).unwrap();
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn encrypt_aes128(user_pw: &str) -> Vec<u8> {
        use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
        use lopdf::{EncryptionState, EncryptionVersion, Permissions};
        use std::collections::BTreeMap;
        use std::sync::Arc;
        let mut doc = Document::load_mem(FICHA).unwrap();
        let cf: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
        let version = EncryptionVersion::V4 {
            document: &doc,
            encrypt_metadata: true,
            crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), cf)]),
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner",
            user_password: user_pw,
            permissions: Permissions::all(),
        };
        let state = EncryptionState::try_from(version).unwrap();
        doc.encrypt(&state).unwrap();
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn encrypt_aes256(user_pw: &str) -> Vec<u8> {
        use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
        use lopdf::{EncryptionState, EncryptionVersion, Permissions};
        use std::collections::BTreeMap;
        use std::sync::Arc;
        let mut doc = Document::load_mem(FICHA).unwrap();
        let cf: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
        let key = [0x42u8; 32];
        let version = EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), cf)]),
            file_encryption_key: &key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner",
            user_password: user_pw,
            permissions: Permissions::all(),
        };
        let state = EncryptionState::try_from(version).unwrap();
        doc.encrypt(&state).unwrap();
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Generate the committed encrypted fixtures. Ignored so routine `cargo test`
    /// doesn't overwrite them. Run on demand:
    ///   cargo test emit_encrypted_fixtures -- --ignored
    #[test]
    #[ignore]
    fn emit_encrypted_fixtures() {
        use std::path::Path;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/generated");
        for (name, bytes) in [
            ("ficha-rc4.pdf", encrypt_rc4("")),
            ("ficha-aes128.pdf", encrypt_aes128("")),
            ("ficha-aes256.pdf", encrypt_aes256("")),
            ("ficha-rc4-pw.pdf", encrypt_rc4("secret")),
        ] {
            // Self-check: each fixture must round-trip through decrypt before commit.
            let mut reloaded = Document::load_mem(&bytes).unwrap();
            assert!(reloaded.trailer.has(b"Encrypt"), "{name} should be encrypted");
            let pw = if name == "ficha-rc4-pw.pdf" { "secret" } else { "" };
            reloaded.decrypt(pw).unwrap_or_else(|e| panic!("{name} decrypt failed: {e}"));
            std::fs::write(dir.join(name), &bytes).expect("write fixture");
        }
    }
```

- [ ] **Step 2: Generate the fixtures**

Run: `cd crates/core && cargo test emit_encrypted_fixtures -- --ignored`
Expected: PASS (the self-checks decrypt each fixture), and the four files now exist.
Then run: `ls -1 tests/fixtures/generated/ficha-rc4.pdf tests/fixtures/generated/ficha-aes128.pdf tests/fixtures/generated/ficha-aes256.pdf tests/fixtures/generated/ficha-rc4-pw.pdf`
Expected: all four listed.

If `encrypt_aes256` (V5) fails to construct or round-trip, delete the `ficha-aes256.pdf` tuple line and its later test (Step 4's `decrypts_aes256_empty_password`); RC4 + AES-128 are the required minimum per the spec. Note this deviation in the report.

- [ ] **Step 3: Write the failing `decrypt_pdf` unit tests**

Add to the same `tests` module (the `include_bytes!` requires the fixtures from Step 2 to exist):

```rust
    const FICHA_RC4: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-rc4.pdf");
    const FICHA_AES128: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-aes128.pdf");
    const FICHA_AES256: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-aes256.pdf");
    const FICHA_RC4_PW: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-rc4-pw.pdf");

    fn assert_decrypted_ficha(out: &[u8]) {
        let doc = Document::load_mem(out).unwrap();
        assert!(!doc.trailer.has(b"Encrypt"), "decrypted output must not be encrypted");
        let fields = crate::forms::read_fields_json(out).unwrap();
        assert!(fields.contains("beneficiario.apellidos_nombres"), "fields should be readable");
    }

    #[test]
    fn decrypt_pdf_passes_through_unencrypted_unchanged() {
        let out = decrypt_pdf(FICHA, "").unwrap();
        assert_eq!(out, FICHA, "unencrypted input must be returned byte-identical");
    }

    #[test]
    fn decrypts_rc4_empty_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_RC4, "").unwrap());
    }

    #[test]
    fn decrypts_aes128_empty_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_AES128, "").unwrap());
    }

    #[test]
    fn decrypts_aes256_empty_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_AES256, "").unwrap());
    }

    #[test]
    fn decrypts_with_correct_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_RC4_PW, "secret").unwrap());
    }

    #[test]
    fn wrong_password_yields_password_prefix() {
        let err = decrypt_pdf(FICHA_RC4_PW, "wrong").unwrap_err();
        assert!(err.starts_with(PASSWORD_PREFIX), "got: {err}");
    }

    #[test]
    fn empty_password_on_password_protected_yields_password_prefix() {
        let err = decrypt_pdf(FICHA_RC4_PW, "").unwrap_err();
        assert!(err.starts_with(PASSWORD_PREFIX), "got: {err}");
    }
```

- [ ] **Step 4: Run the tests — verify they FAIL**

Run: `cd crates/core && cargo test decrypt_pdf decrypts_ wrong_password empty_password_on`
Expected: compile error — `decrypt_pdf` and `PASSWORD_PREFIX` are not defined yet.

- [ ] **Step 5: Implement `decrypt_pdf` + `PASSWORD_PREFIX`**

In `crates/core/src/doc_io.rs` (module scope, next to `ENCRYPTED_PREFIX` and `load_pdf`):

```rust
/// Stable, machine-detectable prefix the TS boundary maps to `IncorrectPasswordError`.
pub const PASSWORD_PREFIX: &str = "PASSWORD:";

/// Decrypt `data` using `password` (empty string handles the common owner-locked
/// case). Unencrypted input is returned verbatim (byte-identical). Encrypted
/// input is decrypted in place, the `/Encrypt` trailer entry removed, and the
/// document re-serialized to plaintext bytes. A wrong/missing password yields an
/// error starting with [`PASSWORD_PREFIX`]; an unsupported or unreadable scheme
/// yields one starting with [`ENCRYPTED_PREFIX`].
pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    if !doc.trailer.has(b"Encrypt") {
        return Ok(data.to_vec());
    }
    match doc.decrypt(password) {
        Ok(()) => {
            doc.trailer.remove(b"Encrypt");
            let mut out = Vec::new();
            doc.save_to(&mut out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        Err(lopdf::Error::Decryption(de)) => Err(classify_decryption_error(de)),
        // The trailer had /Encrypt but lopdf reports it isn't really encrypted:
        // treat as plaintext.
        Err(lopdf::Error::NotEncrypted) => Ok(data.to_vec()),
        Err(e) => Err(format!("{ENCRYPTED_PREFIX} {e}")),
    }
}

/// Map a lopdf decryption error to one of our stable prefixes.
fn classify_decryption_error(de: lopdf::encryption::DecryptionError) -> String {
    use lopdf::encryption::DecryptionError::{
        IncorrectPassword, MissingOwnerPassword, MissingUserPassword, Padding,
    };
    match de {
        IncorrectPassword | Padding | MissingUserPassword | MissingOwnerPassword => {
            format!("{PASSWORD_PREFIX} incorrect or missing password for this encrypted PDF")
        }
        other => format!("{ENCRYPTED_PREFIX} unsupported or unreadable encryption: {other}"),
    }
}
```

- [ ] **Step 6: Run the unit tests — expect PASS**

Run: `cd crates/core && cargo test decrypt_pdf decrypts_ wrong_password empty_password_on`
Expected: all PASS.

- [ ] **Step 7: Export `decrypt_pdf` from `lib.rs`**

In `crates/core/src/lib.rs`, next to the other `#[wasm_bindgen]` exports (e.g. after `read_fields`):

```rust
/// Decrypt an encrypted PDF with `password` (empty string for the common
/// owner-locked case) and return plaintext bytes. Unencrypted input is returned
/// unchanged. Errors start with `PASSWORD:` (bad/missing password) or
/// `ENCRYPTED:` (unsupported scheme).
#[wasm_bindgen]
pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, JsError> {
    doc_io::decrypt_pdf(data, password).map_err(|e| JsError::new(&e))
}
```

(`doc_io` is already a module in `lib.rs`; if its declaration is `mod doc_io;` it is reachable here. The function is `pub`.)

- [ ] **Step 8: Run the full crate suite + warning check**

Run: `cd crates/core && cargo test`
Expected: all pass (new tests + existing, including `load_pdf_rejects_encrypted_trailer`). 0 warnings from `cargo build`.

- [ ] **Step 9: Commit**

```bash
git add crates/core/src/doc_io.rs crates/core/src/lib.rs tests/fixtures/generated/ficha-rc4.pdf tests/fixtures/generated/ficha-aes128.pdf tests/fixtures/generated/ficha-aes256.pdf tests/fixtures/generated/ficha-rc4-pw.pdf
git commit -m "feat(core): decrypt encrypted PDFs (RC4/AES) on a new decrypt_pdf entry point

Add doc_io::decrypt_pdf + wasm export: unencrypted input passes through
byte-identical; encrypted input is decrypted via lopdf, /Encrypt stripped,
and re-serialized to plaintext. Wrong/missing password -> PASSWORD: prefix;
unsupported scheme -> ENCRYPTED:. Adds RC4/AES-128/AES-256 test fixtures.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: TS — decrypt at load, password option, typed error

**Files:**
- Modify: `src/core/wasm.ts` (import `decrypt_pdf`, export `decryptPdf` wrapper)
- Modify: `src/core/wasm-browser.ts` (same)
- Modify: `src/core/document.ts` (add `decryptPdf` to the `CoreWasm` interface)
- Modify: `src/core/errors.ts` (`IncorrectPasswordError`, `toPdfError` mapping, `EncryptedPdfError` message)
- Modify: `src/index.ts` (`PdfDocument.load` gains `opts`, calls `decryptPdf`; export `IncorrectPasswordError`)
- Modify: `src/index.browser.ts` (export `IncorrectPasswordError`)
- Test: `tests/encrypted.test.ts` (new)

**Interfaces:**
- Consumes: the Rust `decrypt_pdf(data, password) -> bytes` from Task 1.
- Produces: `PdfDocument.load(input, opts?: { password?: string })`; `CoreWasm.decryptPdf(data: Uint8Array, password: string): Uint8Array`; `IncorrectPasswordError extends PdfError`.

- [ ] **Step 1: Rebuild WASM (pick up Task 1)**

Run: `bun run build:wasm`
Expected: `✨ Done`.

- [ ] **Step 2: Write the failing TS tests**

Create `tests/encrypted.test.ts`. **Note the opt-in semantics:** decryption only
happens when a `password` is passed (even `""`); bare `load(bytes)` stays lazy and
an encrypted file then rejects on first use.

```ts
import { test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument, IncorrectPasswordError, EncryptedPdfError } from "../src/index.ts";

const fx = (name: string) =>
  new Uint8Array(readFileSync(join(import.meta.dir, "fixtures/generated", name)));

test("loads an RC4-encrypted PDF with an explicit empty password", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"), { password: "" });
  const names = doc.getForm().getFields().map((f) => f.name);
  expect(names).toContain("beneficiario.apellidos_nombres");
});

test("loads an AES-128-encrypted PDF with an explicit empty password", async () => {
  const doc = await PdfDocument.load(fx("ficha-aes128.pdf"), { password: "" });
  expect(doc.getForm().getFields().length).toBeGreaterThan(0);
});

test("loads a password-protected PDF with the correct password", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "secret" });
  expect(doc.getForm().getFields().length).toBeGreaterThan(0);
});

test("wrong password throws IncorrectPasswordError", async () => {
  await expect(
    PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "wrong" }),
  ).rejects.toBeInstanceOf(IncorrectPasswordError);
});

test("empty password on a password-protected PDF throws IncorrectPasswordError", async () => {
  await expect(
    PdfDocument.load(fx("ficha-rc4-pw.pdf"), { password: "" }),
  ).rejects.toBeInstanceOf(IncorrectPasswordError);
});

test("an encrypted PDF loaded without a password rejects on use (opt-in)", async () => {
  // Bare load is lazy; the existing reject fires on the first operation.
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"));
  expect(() => doc.getForm().getFields()).toThrow(EncryptedPdfError);
});

test("filling an encrypted form produces a decrypted output", async () => {
  const doc = await PdfDocument.load(fx("ficha-rc4.pdf"), { password: "" });
  doc.getForm().getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
  const out = await doc.save();
  // Reload WITHOUT a password — the output must be plain (decrypted).
  const reloaded = await PdfDocument.load(out);
  expect(reloaded.getForm().getField("beneficiario.apellidos_nombres")?.value).toBe("GARCIA");
});
```

- [ ] **Step 3: Run the tests — verify they FAIL**

Run: `bun test tests/encrypted.test.ts`
Expected: FAIL — `IncorrectPasswordError` isn't exported yet and `load` doesn't accept `opts`/decrypt.

- [ ] **Step 4: Add the `decryptPdf` wrapper to both wasm bindings**

In `src/core/wasm.ts`: add `decrypt_pdf` to the import list from `../../pkg-web/better_pdf_core.js`, and add:

```ts
export function decryptPdf(data: Uint8Array, password: string): Uint8Array {
  return decrypt_pdf(data, password);
}
```

In `src/core/wasm-browser.ts`: add `decrypt_pdf` to its import list and add the wrapper, calling the file's `ensureInitialized()` guard exactly as the sibling wrappers (`readFields`, `fillFields`) do:

```ts
export function decryptPdf(data: Uint8Array, password: string): Uint8Array {
  ensureInitialized();
  return decrypt_pdf(data, password);
}
```

- [ ] **Step 5: Add `decryptPdf` to the `CoreWasm` interface**

In `src/core/document.ts`, in the `export interface CoreWasm { … }` block, add:

```ts
  decryptPdf(data: Uint8Array, password: string): Uint8Array;
```

- [ ] **Step 6: Add `IncorrectPasswordError` and the mapping**

In `src/core/errors.ts`, after `EncryptedPdfError`, add:

```ts
/** Thrown when an encrypted PDF's password is wrong or missing. Pass the
 * correct password via `PdfDocument.load(bytes, { password })`. */
export class IncorrectPasswordError extends PdfError {
  constructor(
    message = "incorrect or missing password for this encrypted PDF",
  ) {
    super(message);
  }
}
```

Update `EncryptedPdfError`'s default message to point the caller at the password option (it now also fires when an encrypted file is loaded without one):

```ts
  constructor(
    message = "this PDF is encrypted; load it with PdfDocument.load(bytes, { password }) (use \"\" for owner-locked files)",
  ) {
```

Update `toPdfError` to map the password prefix (before the encrypted one):

```ts
  if (message.includes("PASSWORD:")) return new IncorrectPasswordError();
  if (message.includes("ENCRYPTED:")) return new EncryptedPdfError();
```

- [ ] **Step 7: Update `PdfDocument.load` and export the error**

In `src/index.ts`, replace `static async load`. **Decryption is opt-in:** when no
`password` is given, stay lazy and unchanged (no WASM call — preserves the
benchmark hot path); only decrypt when a `password` is provided.

```ts
  static async load(
    input: Uint8Array | ArrayBuffer,
    opts?: { password?: string },
  ): Promise<PdfDocument> {
    const raw = input instanceof Uint8Array ? input : new Uint8Array(input);
    if (opts?.password === undefined) {
      return new PdfDocument(raw, wasm);
    }
    let bytes: Uint8Array;
    try {
      bytes = wasm.decryptPdf(raw, opts.password);
    } catch (e) {
      throw toPdfError(e);
    }
    return new PdfDocument(bytes, wasm);
  }
```

Ensure `toPdfError` is imported in `src/index.ts` (add it to the existing import from `./core/errors.js` if absent). Add `IncorrectPasswordError` to the error re-export list in `src/index.ts`.

Then apply the **same opt-in change** to `src/index.browser.ts`, whose `PdfDocument.load` is a separate implementation (it already `await initializeWasm()` first):

```ts
  static async load(
    input: Uint8Array | ArrayBuffer,
    opts?: { password?: string },
  ): Promise<PdfDocument> {
    await initializeWasm();
    const raw = input instanceof Uint8Array ? input : new Uint8Array(input);
    if (opts?.password === undefined) {
      return new PdfDocument(raw, wasm);
    }
    let bytes: Uint8Array;
    try {
      bytes = wasm.decryptPdf(raw, opts.password);
    } catch (e) {
      throw toPdfError(e);
    }
    return new PdfDocument(bytes, wasm);
  }
```

Import `toPdfError` in `src/index.browser.ts` and add `IncorrectPasswordError` to its error re-export list.

- [ ] **Step 8: Run the tests — expect PASS**

Run: `bun test tests/encrypted.test.ts`
Expected: all PASS.

- [ ] **Step 9: Typecheck + full suite**

Run: `bun run typecheck && bun test`
Expected: typecheck clean; full suite green (0 fail), including the existing no-op-save byte-identity test.

- [ ] **Step 10: Commit**

```bash
git add src/core/wasm.ts src/core/wasm-browser.ts src/core/document.ts src/core/errors.ts src/index.ts src/index.browser.ts tests/encrypted.test.ts
git commit -m "feat(core): decrypt encrypted PDFs at load with optional password

PdfDocument.load(bytes, { password? }) now decrypts encrypted PDFs (empty
password by default) via the decrypt_pdf WASM entry point, caching plaintext
bytes so all downstream ops and saves are unchanged. Wrong/missing password
throws the new IncorrectPasswordError; modifying yields a decrypted output.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Docs + changelog + release

**Files:**
- Modify: `docs/site/src/content/docs/reference/limitations.md`
- Modify: `docs/site/src/content/docs/guides/filling-forms.md`
- Modify: `docs/site/src/content/docs/migrating/from-pdf-lib.md`
- Modify: `README.md`
- Modify: `CHANGELOG.md`, `package.json`, `crates/core/Cargo.toml`, `crates/core/Cargo.lock`

**Interfaces:** none (docs only).

- [ ] **Step 1: Update `limitations.md`**

Replace the `No encrypted PDF support` bullet with a residual-only limitation:

```md
- **Encrypted PDFs are decrypted on load** (RC4, AES-128, AES-256) when you pass a
  password: `PdfDocument.load(bytes, { password })`. Use `{ password: "" }` for
  owner-locked / empty-user-password files. (Decryption is opt-in — bare
  `load(bytes)` does not decrypt, so an encrypted file loaded without a password
  throws `EncryptedPdfError` telling you to pass one; a wrong password throws
  `IncorrectPasswordError`.) Modifying an encrypted PDF produces a **decrypted**
  output. **Still unsupported:** producing encrypted output (re-encryption) and
  encrypting documents you create.
```

- [ ] **Step 2: Document loading in `filling-forms.md`**

Add a short section (after the intro / before "Inspect fields"):

```md
## Encrypted PDFs

`PdfDocument.load` decrypts encrypted PDFs (RC4 / AES-128 / AES-256) when you pass
a `password`. Use `""` for owner-locked files (an empty user password):

```ts
const ownerLocked = await PdfDocument.load(bytes, { password: "" });
const protected_ = await PdfDocument.load(bytes, { password: "secret" });
```

Decryption is opt-in: bare `load(bytes)` does not decrypt, so an encrypted file
loaded without a `password` throws `EncryptedPdfError` (pass a password). A wrong
password throws `IncorrectPasswordError`. Saving an edited encrypted PDF produces
a **decrypted** (unencrypted) output.
```

- [ ] **Step 3: Update the pdf-lib migration note**

In `docs/site/src/content/docs/migrating/from-pdf-lib.md`, add a bullet/section noting the difference:

```md
- **Encrypted PDFs:** pdf-lib throws `EncryptedPDFError` (or with
  `ignoreEncryption: true` skips the check *without* decrypting, yielding garbage
  on save). better-pdf actually decrypts — `PdfDocument.load(bytes, { password })`
  (use `""` for owner-locked files) — and produces a decrypted output when you
  save.
```

- [ ] **Step 4: Update `README.md`**

Find the encrypted-PDF limitation bullet (grep `encrypted`) and the status line claim about "encrypted-PDF detection"; update both to state that encrypted PDFs (RC4/AES) are decrypted on load via `load(bytes, { password })`, output is decrypted, and re-encryption/creating encrypted PDFs is unsupported.

- [ ] **Step 5: CHANGELOG entry + release bump**

Add under `## [Unreleased]`:

```md
### Added

- **Read & modify encrypted PDFs.** `PdfDocument.load(bytes, { password })`
  decrypts RC4 / AES-128 / AES-256 encrypted PDFs (use `""` for owner-locked
  files), so they can be read, filled, and flattened. Decryption is opt-in —
  bare `load(bytes)` is unchanged. Modifying an encrypted PDF produces a
  decrypted output. A wrong password throws the new `IncorrectPasswordError`; an
  encrypted file loaded without a password throws `EncryptedPdfError`. Producing
  encrypted output is still unsupported.
```

Insert `## [1.7.0] - 2026-06-28` between `## [Unreleased]` and the new `### Added`. Bump `package.json` `version` and `crates/core/Cargo.toml` `version` to `1.7.0`, then `cd crates/core && cargo build` to refresh `Cargo.lock`.

- [ ] **Step 6: Verify + commit**

Run: `bun run build:wasm && bun run typecheck && bun test && (cd crates/core && cargo test)`
Expected: all green.

```bash
git add docs README.md CHANGELOG.md package.json crates/core/Cargo.toml crates/core/Cargo.lock
git commit -m "docs(core): document encrypted-PDF support; release 1.7.0

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Notes for the implementer

- `EncryptionState` is owned (no lifetime), so building it from a version that borrows `&doc` releases the borrow before `doc.encrypt(&state)` — no borrow-checker conflict.
- lopdf's `Document::encrypt` encrypts objects in place and sets the trailer `/Encrypt`; its writer does **not** re-encrypt at save time, so `encrypt()` then `save_to()` yields a valid encrypted file.
- Keep the existing `load_pdf` `/Encrypt` reject untouched — it is the safety net for raw-bytes entry points (merge/assemble) that don't pass through `load`.
- Do not thread a password through any other WASM entry point — decryption happens once at `load`, and all downstream code consumes the cached plaintext `this.bytes`.
