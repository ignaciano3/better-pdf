use lopdf::{Document, Object};

/// Stable, machine-detectable prefix the TS boundary maps to `EncryptedPdfError`.
pub const ENCRYPTED_PREFIX: &str = "ENCRYPTED:";

/// Stable, machine-detectable prefix the TS boundary maps to `IncorrectPasswordError`.
pub const PASSWORD_PREFIX: &str = "PASSWORD:";

/// Decrypt `data` using `password` (empty string handles the common owner-locked
/// case). Unencrypted input is returned verbatim (byte-identical). Encrypted
/// input is decrypted in place, the `/Encrypt` trailer entry removed, and the
/// document re-serialized to plaintext bytes. A wrong/missing password yields an
/// error starting with [`PASSWORD_PREFIX`]; an unsupported or unreadable scheme
/// yields one starting with [`ENCRYPTED_PREFIX`].
///
/// In lopdf 0.41, `load_mem_with_options` auto-decrypts during loading when the
/// password authenticates, so we use `was_encrypted()` to distinguish the
/// unencrypted pass-through case from a successfully decrypted document.
pub fn decrypt_pdf(data: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let load_result =
        Document::load_mem_with_options(data, lopdf::LoadOptions::with_password(password));
    match load_result {
        Ok(doc) if !doc.was_encrypted() => {
            // Unencrypted input — return byte-identical.
            Ok(data.to_vec())
        }
        Ok(mut doc) => {
            // Encrypted and successfully decrypted. lopdf already removes /Encrypt
            // during load, but strip it explicitly too so the plaintext contract
            // is enforced here rather than relying on that side effect (and so the
            // writer can't re-encrypt).
            doc.trailer.remove(b"Encrypt");
            let mut out = Vec::new();
            doc.save_to(&mut out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        Err(lopdf::Error::InvalidPassword) => {
            // lopdf mis-derives a 40-bit key for a V4 /Encrypt dict that omits
            // the top-level /Length (spec fixes V4 at 128-bit). If this is that
            // shape, inject /Length 128 and retry once before reporting a bad
            // password. The retry only runs on an already-failed decrypt, so it
            // cannot affect well-formed files.
            if let Some(fixed) = crate::repair::inject_v4_length(data)
                && let Ok(mut doc) = Document::load_mem_with_options(
                    &fixed,
                    lopdf::LoadOptions::with_password(password),
                )
                && doc.was_encrypted()
            {
                doc.trailer.remove(b"Encrypt");
                let mut out = Vec::new();
                doc.save_to(&mut out).map_err(|e| e.to_string())?;
                return Ok(out);
            }
            Err(format!(
                "{PASSWORD_PREFIX} incorrect or missing password for this encrypted PDF"
            ))
        }
        // Defensive / forward-compatible: lopdf 0.41 surfaces wrong passwords as
        // the load-time `InvalidPassword` above, but retain this arm in case a
        // future version raises a `Decryption` error during loading instead.
        Err(lopdf::Error::Decryption(de)) => Err(classify_decryption_error(de)),
        Err(e) => Err(format!("{ENCRYPTED_PREFIX} {e}")),
    }
}

/// True when `data` is an encrypted PDF, without attempting to decrypt or
/// requiring a password. Lets callers branch (e.g. prompt for a password)
/// instead of catching a throw on first use.
///
/// Detection is robust to lopdf's eager decryption: a normal parse reports it
/// via the trailer `/Encrypt` or `was_encrypted()` (the latter catches
/// empty-password files whose `/Encrypt` lopdf already stripped); a parse that
/// fails for a wrong/absent password is by definition encrypted; and a parse
/// that fails for other reasons (broken xref) falls back to scanning the raw
/// bytes for an `/Encrypt` trailer reference.
pub fn is_encrypted(data: &[u8]) -> bool {
    match Document::load_mem(data) {
        Ok(doc) => doc.trailer.has(b"Encrypt") || doc.was_encrypted(),
        Err(lopdf::Error::InvalidPassword) => true,
        Err(_) => crate::repair::has_encrypt_marker(data),
    }
}

/// Classify how `password` authorizes an encrypted PDF: `"owner"` (full
/// access), `"user"` (restricted access), or `None` when it authenticates
/// neither role (wrong password) or the document isn't encrypted / can't be
/// probed. Owner is reported when the owner check passes, even if the user
/// check would too (owner grants a superset of access) — matching pypdf's
/// `PasswordType`.
///
/// lopdf's loader eagerly decrypts and strips `/Encrypt`, and its
/// `authenticate_*_password` need a retained `/Encrypt`; so we authenticate
/// against a minimal plaintext probe carrying just the `/Encrypt` dict and
/// `/ID` (`repair::build_encrypt_probe`) rather than the live document.
/// Limited to classic-`trailer` files — xref-stream encrypted files return
/// `None`.
pub fn password_type(data: &[u8], password: &str) -> Option<&'static str> {
    let probe = crate::repair::build_encrypt_probe(data)?;
    let mut doc = Document::load_mem(&probe).ok()?;
    // The probe deliberately omits /Encrypt from its trailer (so lopdf loads it
    // as plaintext); wire it back to the embedded dict for authentication.
    doc.trailer.set("Encrypt", Object::Reference((1, 0)));
    if doc.authenticate_owner_password(password).is_ok() {
        return Some("owner");
    }
    if doc.authenticate_user_password(password).is_ok() {
        return Some("user");
    }
    None
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

/// True when the trailer /Root resolves to a usable page tree: a catalog whose
/// `/Pages` entry both exists and leads to real pages. A file can parse yet
/// point `/Pages` at the wrong object (e.g. the Info dict — pypdf iss2516), in
/// which case the strict loader succeeds but no page resolves; we treat that as
/// invalid so the recovery loader can re-point `/Pages` at the true page tree.
fn root_is_valid(doc: &Document) -> bool {
    let Some(root_dict) = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_dictionary(id).ok())
    else {
        return false;
    };
    if !root_dict.has(b"Pages") {
        return false;
    }
    // Accept when /Pages resolves to a page-tree node, or when the document
    // otherwise yields at least one page (tolerating producers that omit the
    // /Type /Pages marker). Reject when neither holds — the page tree is broken.
    let pages_typed = root_dict
        .get(b"Pages")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        })
        .and_then(|d| d.get(b"Type").ok())
        .and_then(|o| o.as_name().ok())
        == Some(b"Pages");
    pages_typed || !doc.get_pages().is_empty()
}

/// Parse PDF bytes into a `Document`, failing fast on encrypted files.
///
/// Rejects any originally-encrypted PDF so the operation fails loudly rather than
/// silently corrupting on save. Two cases must both be caught:
/// - A still-encrypted trailer (`/Encrypt` present) — e.g. a password-protected
///   file loaded without the password.
/// - An auto-decrypted file: lopdf 0.41's `load_mem` transparently decrypts a
///   file whose user password is empty and **removes** `/Encrypt`, so the trailer
///   check alone misses it. `was_encrypted()` still reports `true`. Without this
///   the incremental save path would append plaintext onto the original encrypted
///   bytes and produce a broken document.
///
/// Callers decrypt up front with [`decrypt_pdf`] (via `PdfDocument.load(bytes,
/// { password })`), whose plaintext output reports `was_encrypted() == false` and
/// passes this check.
pub fn load_pdf(data: &[u8]) -> Result<Document, String> {
    let doc = match Document::load_mem(data) {
        Ok(doc) => doc,
        // Strict parse failed (broken xref/trailer, junk before header, …):
        // fall back to the recovery loader. Only this error path pays the
        // repair cost; well-formed files never reach it.
        Err(primary) => crate::repair::repair_load(data).map_err(|repair_err| {
            // repair_load rejects originally-encrypted PDFs (broken xref, but
            // still ciphertext) with an ENCRYPTED_PREFIX error before it ever
            // gets to rebuild a "plaintext" trailer. That must propagate as-is
            // rather than being swallowed by the primary parse error below, or
            // callers would silently accept a still-encrypted document.
            if repair_err.starts_with(ENCRYPTED_PREFIX) {
                repair_err
            } else {
                primary.to_string()
            }
        })?,
    };
    // Check encryption first, before validating root, so encrypted documents
    // are rejected regardless of root validity.
    if doc.trailer.has(b"Encrypt") || doc.was_encrypted() {
        return Err(format!(
            "{ENCRYPTED_PREFIX} this PDF is encrypted; load it with PdfDocument.load(bytes, {{ password }}) (use \"\" for owner-locked files)"
        ));
    }
    // A parse can "succeed" with a /Root pointing at a non-catalog object
    // (pdf-lib's invalid_root_ref.pdf). Treat that as a failed parse too.
    let doc = if root_is_valid(&doc) {
        doc
    } else {
        crate::repair::repair_load(data).map_err(|repair_err| {
            // Same rationale as the primary-parse-failure arm above: a
            // still-encrypted document with an invalid /Root must be reported
            // as encrypted, not masked behind the generic repair-failed message.
            if repair_err.starts_with(ENCRYPTED_PREFIX) {
                repair_err
            } else {
                "invalid /Root reference and repair failed".to_string()
            }
        })?
    };
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Document, Object, dictionary};

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
            // Password-protected (non-empty user password) AES fixtures so the
            // wrong-password path is exercisable: lopdf tries the empty password
            // first, so only a non-empty user password rejects a wrong guess.
            ("ficha-aes128-pw.pdf", encrypt_aes128("secret")),
            ("ficha-aes256-pw.pdf", encrypt_aes256("secret")),
        ] {
            // Self-check: each fixture must round-trip through decrypt before commit.
            // In lopdf 0.41, load_mem auto-decrypts when the password is available;
            // supply the right password via load_mem_with_options so was_encrypted() is true.
            let pw = if name.ends_with("-pw.pdf") {
                "secret"
            } else {
                ""
            };
            let loaded =
                Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(pw))
                    .unwrap_or_else(|e| panic!("{name} decrypt failed: {e}"));
            assert!(
                loaded.was_encrypted(),
                "{name} should have been encrypted before loading"
            );
            std::fs::write(dir.join(name), &bytes).expect("write fixture");
        }
    }

    /// Build a minimal valid PDF whose trailer references an `/Encrypt` dict,
    /// without performing real encryption (detection only checks the key).
    fn encrypted_pdf_bytes() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        // A dummy /Encrypt dictionary referenced from the trailer.
        let mut enc = Dictionary::new();
        enc.set("Filter", Object::Name(b"Standard".to_vec()));
        enc.set("V", 1);
        enc.set("R", 2);
        let enc_id = doc.add_object(Object::Dictionary(enc));
        doc.trailer.set("Root", catalog_id);
        doc.trailer.set("Encrypt", Object::Reference(enc_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn plain_pdf_bytes() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn load_pdf_rejects_encrypted_trailer() {
        let bytes = encrypted_pdf_bytes();
        let err = load_pdf(&bytes).expect_err("encrypted PDF must be rejected");
        assert!(
            err.starts_with(ENCRYPTED_PREFIX),
            "error must start with ENCRYPTED prefix, got: {err}"
        );
    }

    #[test]
    fn load_pdf_accepts_plain_pdf() {
        let bytes = plain_pdf_bytes();
        assert!(load_pdf(&bytes).is_ok(), "plain PDF must load");
    }

    #[test]
    fn load_pdf_rejects_auto_decrypted_empty_password_file() {
        // lopdf auto-decrypts an empty-user-password file during load_mem and
        // strips /Encrypt, so the trailer check alone misses it. load_pdf must
        // still reject via was_encrypted(), or a later incremental save would
        // append plaintext onto the encrypted base and silently corrupt output.
        let err = load_pdf(FICHA_RC4).expect_err("auto-decrypted encrypted PDF must be rejected");
        assert!(err.starts_with(ENCRYPTED_PREFIX), "got: {err}");
    }

    #[test]
    fn load_pdf_accepts_decrypt_pdf_output() {
        // The plaintext output of decrypt_pdf must pass load_pdf (was_encrypted=false).
        let plain = decrypt_pdf(FICHA_RC4, "").unwrap();
        assert!(load_pdf(&plain).is_ok(), "decrypted output must load");
    }

    const FICHA_RC4: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-rc4.pdf");
    const FICHA_AES128: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-aes128.pdf");
    const FICHA_AES256: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-aes256.pdf");
    const FICHA_RC4_PW: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-rc4-pw.pdf");
    const FICHA_AES128_PW: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-aes128-pw.pdf");
    const FICHA_AES256_PW: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-aes256-pw.pdf");

    fn assert_decrypted_ficha(out: &[u8]) {
        let doc = Document::load_mem(out).unwrap();
        assert!(
            !doc.trailer.has(b"Encrypt"),
            "decrypted output must not be encrypted"
        );
        let fields = crate::forms::read_fields_json(out).unwrap();
        assert!(
            fields.contains("beneficiario.apellidos_nombres"),
            "fields should be readable"
        );
    }

    #[test]
    fn decrypt_pdf_passes_through_unencrypted_unchanged() {
        let out = decrypt_pdf(FICHA, "").unwrap();
        assert_eq!(
            out, FICHA,
            "unencrypted input must be returned byte-identical"
        );
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

    const AESV2_NO_LENGTH: &[u8] =
        include_bytes!("../../../tests/fixtures/pypdf/encryption/r4-aes-v2-no-key-length.pdf");

    #[test]
    fn decrypts_v4_aes128_missing_length_entry() {
        // A V4 /Encrypt dict without a top-level /Length: lopdf mis-derives a
        // 40-bit key and rejects the password; the /Length-128 injection retry
        // recovers it. Output must be plaintext and readable.
        let out = decrypt_pdf(AESV2_NO_LENGTH, "").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert!(!doc.trailer.has(b"Encrypt"), "must decrypt to plaintext");
        assert_eq!(doc.get_pages().len(), 1);
        // /Info survives so metadata is intact (the fixture's author is "cheng").
        assert!(load_pdf(&out).is_ok(), "decrypted output must reload");
    }

    #[test]
    fn is_encrypted_detects_encrypted_and_plain() {
        // Plain and created documents are not encrypted.
        assert!(!is_encrypted(&plain_pdf_bytes()));
        assert!(!is_encrypted(FICHA));
        // Empty-user-password files (lopdf strips /Encrypt on load, was_encrypted
        // stays true), password-protected files, and a V4-missing-/Length file
        // are all detected as encrypted without a password.
        assert!(is_encrypted(FICHA_RC4));
        assert!(is_encrypted(FICHA_AES256));
        assert!(is_encrypted(FICHA_RC4_PW));
        assert!(is_encrypted(AESV2_NO_LENGTH));
    }

    #[test]
    fn is_encrypted_false_on_garbage() {
        assert!(!is_encrypted(b"not a pdf at all"));
    }

    fn enc(file: &str) -> Vec<u8> {
        std::fs::read(format!("../../tests/fixtures/pypdf/encryption/{file}")).unwrap()
    }

    #[test]
    fn password_type_classifies_user_vs_owner() {
        // Genuinely distinct non-empty passwords (user=foo, owner=bar).
        assert_eq!(password_type(&enc("r6-both-passwords.pdf"), "bar"), Some("owner"));
        assert_eq!(password_type(&enc("r6-both-passwords.pdf"), "foo"), Some("user"));
        // Wrong password authenticates neither role.
        assert_eq!(password_type(&enc("r6-both-passwords.pdf"), "nope"), None);
    }

    #[test]
    fn password_type_reports_owner_for_owner_password_and_user_for_user() {
        // owner="asdfzxcv", empty user: the owner password → owner, "" → user.
        assert_eq!(password_type(&enc("r6-owner-password.pdf"), "asdfzxcv"), Some("owner"));
        assert_eq!(password_type(&enc("r6-owner-password.pdf"), ""), Some("user"));
        // user="asdfzxcv", empty owner: the user password → user, "" → owner
        // (an empty owner password grants owner access to any opener).
        assert_eq!(password_type(&enc("r6-user-password.pdf"), "asdfzxcv"), Some("user"));
        assert_eq!(password_type(&enc("r6-user-password.pdf"), ""), Some("owner"));
    }

    #[test]
    fn password_type_none_for_unencrypted() {
        assert_eq!(password_type(&enc("unencrypted.pdf"), ""), None);
        assert_eq!(password_type(&plain_pdf_bytes(), "anything"), None);
    }

    #[test]
    fn inject_v4_length_declines_non_matching_files() {
        // A plain PDF has no /Encrypt, so the fallback must decline (None),
        // never fabricate a rebuild.
        assert!(crate::repair::inject_v4_length(&plain_pdf_bytes()).is_none());
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

    // Wrong-password tests must use *password-protected* (non-empty user
    // password) AES fixtures: lopdf tries the empty password first, so a
    // wrong guess against an owner-locked (empty user password) file would be
    // silently ignored and the file opened. These fixtures have user password
    // "secret", so an empty/wrong guess is genuinely rejected.
    #[test]
    fn wrong_password_on_aes128_yields_password_prefix() {
        let err = decrypt_pdf(FICHA_AES128_PW, "wrong").unwrap_err();
        assert!(err.starts_with(PASSWORD_PREFIX), "got: {err}");
    }

    #[test]
    fn wrong_password_on_aes256_yields_password_prefix() {
        let err = decrypt_pdf(FICHA_AES256_PW, "wrong").unwrap_err();
        assert!(err.starts_with(PASSWORD_PREFIX), "got: {err}");
    }

    // Sanity: the correct password decrypts the password-protected AES fixtures.
    #[test]
    fn decrypts_aes128_with_correct_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_AES128_PW, "secret").unwrap());
    }

    #[test]
    fn decrypts_aes256_with_correct_password() {
        assert_decrypted_ficha(&decrypt_pdf(FICHA_AES256_PW, "secret").unwrap());
    }

    #[test]
    fn recovers_invalid_root_ref() {
        const INVALID_ROOT: &[u8] =
            include_bytes!("../../../tests/fixtures/pdf-lib/invalid_root_ref.pdf");
        let doc = load_pdf(INVALID_ROOT).unwrap();
        assert!(!doc.get_pages().is_empty(), "must recover the real catalog");
    }
}
