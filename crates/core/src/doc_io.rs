use lopdf::Document;

/// Stable, machine-detectable prefix the TS boundary maps to `EncryptedPdfError`.
pub const ENCRYPTED_PREFIX: &str = "ENCRYPTED:";

/// Parse PDF bytes into a `Document`, failing fast on encrypted files.
///
/// Encryption is not supported. If the parsed trailer carries an `/Encrypt`
/// entry, this returns an `Err` whose message starts with [`ENCRYPTED_PREFIX`]
/// so the TS layer can raise a typed `EncryptedPdfError`.
pub fn load_pdf(data: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    if doc.trailer.has(b"Encrypt") {
        return Err(format!(
            "{ENCRYPTED_PREFIX} this PDF is encrypted; encrypted PDFs are not supported"
        ));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary, Document, Object};

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
}
