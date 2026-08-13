---
title: Decrypting PDFs
description: Open password-protected and owner-locked PDFs, and detect encryption before loading.
---

`PdfDocument.load` decrypts encrypted PDFs (RC4 / AES-128 / AES-256) when you pass
a `password`. Use `""` for owner-locked files (an empty user password):

```ts
const ownerLocked = await PdfDocument.load(bytes, { password: "" });
const protected_ = await PdfDocument.load(bytes, { password: "secret" });
```

Decryption is opt-in: bare `load(bytes)` does not decrypt, so an encrypted file
loaded without a `password` throws `EncryptedPdfError` (pass a password). A wrong
password throws `IncorrectPasswordError`.

Saving an edited encrypted PDF produces a **decrypted** (unencrypted) output.
Re-encryption and creating encrypted PDFs are not supported (see
[Limitations](/better-pdf/reference/limitations/)).

## Detect encryption before loading

`PdfDocument.isEncrypted(bytes)` reports whether a PDF is encrypted without
decrypting it or needing a password — use it to decide whether to pass a
`password` to `load`:

```ts
if (await PdfDocument.isEncrypted(bytes)) {
  doc = await PdfDocument.load(bytes, { password });
} else {
  doc = await PdfDocument.load(bytes);
}
```

## Classify a password

`PdfDocument.passwordType(bytes, password)` classifies how a password
authorizes an encrypted PDF without loading it:

```ts
const kind = await PdfDocument.passwordType(bytes, pw); // "owner" | "user" | null
```

- `"owner"` — full access. Reported when the password satisfies the owner check,
  even if it would also satisfy the user check (owner access is a superset).
- `"user"` — restricted access (permission flags from the encryption dictionary
  apply).
- `null` — the password authenticates neither role (wrong password), or the
  document is not an encrypted PDF.

`passwordType` returns `null` for a correct password only in the rare case of an
xref-stream encrypted file that omits the `/Encrypt` entries the check needs —
`load(bytes, { password })` still decrypts those files, so prefer validating by
attempting the load when you control the flow.

## Type generator

The `better-pdf-generate-types` CLI opens encrypted PDFs with `--password PW`
(pass `--password ''` for owner-locked files). See
[Typed forms](/better-pdf/guides/typed-forms/).

## See also

- [Errors](/better-pdf/reference/errors/) — `EncryptedPdfError` and
  `IncorrectPasswordError`.
- [Limitations](/better-pdf/reference/limitations/) — encryption support bounds
  (no re-encryption, no creating encrypted PDFs).
