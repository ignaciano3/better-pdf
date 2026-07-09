# lopdf issue draft: V4 `/Encrypt` without `/Length` decrypts with a 40-bit key

Draft for filing against [lopdf](https://github.com/J-F-Liu/lopdf) (version 0.41.0).
better-pdf ships a local workaround (`repair::inject_v4_length`); this is the
upstream root-cause fix.

---

**Title:** Encrypted V4 (AES-128 / RC4-128) PDFs without a top-level `/Length` fail to decrypt (40-bit key derived instead of 128-bit)

**Version:** lopdf 0.41.0

### Summary

A PDF whose `/Encrypt` dictionary uses `/V 4` but omits the top-level `/Length`
entry cannot be decrypted — `load_mem_with_options(..)` returns
`Error::InvalidPassword` even with the correct (here empty) password. Acrobat,
pdf.js, and pypdf open the same file.

Per ISO 32000-1 §7.6.1, `/V 4` fixes the file-encryption-key length at **128
bits**, so `/Length` is optional for V4 and defaults to 128. lopdf instead
treats a missing `/Length` as 40 bits.

### Root cause

`src/encryption/algorithms.rs`, `PasswordAlgorithm::try_from(&Document)` reads
`length` only from the top-level `/Length` and leaves it `None` when absent (it
never defaults based on `/V`):

```rust
let length: Option<usize> = if encrypted.get(b"Length").is_ok() {
    Some(encrypted.get(b"Length")?.as_i64()?.try_into()?)
} else {
    None
};
```

`compute_file_encryption_key_r4` then falls back to 40 bits:

```rust
let n = if self.revision >= 3 {
    self.length.unwrap_or(40) / 8   // 40/8 = 5 bytes → 40-bit key for a V4 file
} else {
    5
};
```

So for a V4 file without `/Length`, `n = 5` and the derived key is wrong,
yielding `InvalidPassword`.

(Note: `EncryptionState::try_from(EncryptionVersion::V4 { .. })` — the *encrypt*
path — already hardcodes `length: Some(128)`. It's only the *decrypt* path,
built from the parsed dictionary, that misses the default.)

### Minimal reproduction

Any `/V 4` file whose `/Encrypt` dict has no top-level `/Length`, e.g. pypdf's
`resources/encryption/r4-aes-v2-no-key-length.pdf`. The `/Encrypt` dict:

```
<< /CF << /StdCF << /AuthEvent /DocOpen /CFM /AESV2 /Length 16 >> >>
   /Filter /Standard /O <...> /P -4 /R 4 /StmF /StdCF /StrF /StdCF /U <...> /V 4 >>
```

```rust
let doc = Document::load_mem_with_options(bytes, LoadOptions::with_password(""));
// Err(InvalidPassword); expected Ok (1-page document)
```

### Suggested fix

Default `length` from `/V` when the entry is absent, in
`PasswordAlgorithm::try_from`:

```rust
let length: Option<usize> = if encrypted.get(b"Length").is_ok() {
    Some(encrypted.get(b"Length")?.as_i64()?.try_into()?)
} else {
    match version {
        4 => Some(128),
        5 => Some(256),
        _ => None,
    }
};
```

(Equivalently, `compute_file_encryption_key_r4` could default by revision/version
instead of `unwrap_or(40)`.) The existing `Some(length)` validation block already
enforces `length == 128` for V4 / `== 256` for V5, so the defaulted values are
consistent with it.
