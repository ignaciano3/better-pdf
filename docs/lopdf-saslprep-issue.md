# lopdf issue: no raw-bytes load path, so non-normalized Unicode passwords never open

**Status:** not filed upstream yet. Confirmed in lopdf 0.43.0 (also 0.41.0 — the
relevant code is unchanged). better-pdf has **no local workaround**: unlike the
V4 `/Length` bug, this one cannot be fixed from the outside (see below), so
`decrypt_pdf` reports these files as wrong-password and `password_type`
deliberately declines to classify them, keeping the two consistent.

---

**Title:** R5/R6 passwords are always SASLprep-normalized on load, so files keyed off raw UTF-8 bytes cannot be opened

**Version:** lopdf 0.43.0

### Summary

For revision 5/6 (AES-256) encryption, lopdf normalizes the supplied password
with SASLprep before deriving the key. That follows ISO 32000-2 Algorithm 2.A,
but it is unconditional: a file whose producer derived the key from the *raw*
UTF-8 bytes the user typed can never be opened, not even by passing those exact
bytes back.

qpdf is such a producer. A file it encrypts with the NFD spelling of `café`
(`63 61 66 65 CC 81`) opens in qpdf with that password and fails in lopdf with
every spelling:

```bash
python3 -c "import unicodedata,sys; sys.stdout.write(unicodedata.normalize('NFD','café'))" > pw
qpdf --encrypt "$(cat pw)" owner 256 -- base.pdf nfd.pdf
qpdf --check --password="$(cat pw)" nfd.pdf   # ok
qpdf --check --password='café'      nfd.pdf   # invalid password  (NFC ≠ the stored bytes)
```

```rust
let nfd = "cafe\u{301}";
Document::load_mem_with_options(&bytes, LoadOptions::with_password(nfd));
// Err(InvalidPassword); qpdf opens the same file with the same bytes
```

### Root cause

`src/encryption/algorithms.rs`:

```rust
pub(crate) fn sanitize_password_r6(&self, password: &str) -> Result<Vec<u8>, DecryptionError> {
    Ok(stringprep::saslprep(password)?.as_bytes().to_vec())
}
```

There is no path that skips it during **loading**. `LoadOptions` carries a
`String` password, and the reader authenticates through the sanitizing
`Document::authenticate_password`, so the raw bytes never reach key derivation.

The same call also turns SASLprep-prohibited input into a decryption error
rather than a plain authentication failure.

### Why it can't be worked around downstream

lopdf 0.43 does expose raw-bytes entry points — `authenticate_raw_user_password`,
`authenticate_raw_owner_password`, `Document::decrypt_raw` — but they operate on
an already-loaded document, and an encrypted document cannot be loaded without
its password: `Document::load_mem` on an encrypted file returns a husk with the
objects unread (1 object, `was_encrypted() == false`, `/Encrypt` still in the
trailer). Calling `decrypt_raw` on that husk "succeeds" and saves an empty
document — worse than failing. So raw authentication can be *checked* but the
document still cannot be *decrypted*, and reporting a password as valid that we
then cannot open would break the `password_type` ⟺ `decrypt_pdf` invariant.

### Suggested fix

Try the raw bytes as a fallback when the sanitized form fails, in the decrypt
path (`compute_file_encryption_key_r6`, or wherever `sanitize_password_r6` is
called from), matching the spec's allowance for using the password as supplied
when SASLprep does not apply cleanly, and matching what tolerant readers do.

Failing that, a raw-password **load** option — e.g.
`LoadOptions::with_raw_password(impl AsRef<[u8]>)` threaded through
`authenticate_and_setup_encryption` and `EncryptionState::decode` — would let
callers implement the fallback themselves, as they already can for
authentication.

### Fixtures

`tests/scripts/gen-qpdf-fixtures.ts` generates `r6-nfd-password.pdf` and
`r6-nfd-password-xrefstm.pdf` for this case; `tests/qpdf-ported.test.ts` asserts
the current (limited) behavior so the day it starts working is visible.
