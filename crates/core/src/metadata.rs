//! Metadata module: read and incrementally write the PDF Info dictionary.

use lopdf::{Dictionary, Document, IncrementalDocument, Object, StringFormat, decode_text_string};
use serde::{Deserialize, Serialize};

/// Representation of the PDF Info dictionary entries.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(rename = "creationDate", skip_serializing_if = "Option::is_none")]
    pub creation_date: Option<String>,
    #[serde(rename = "modDate", skip_serializing_if = "Option::is_none")]
    pub mod_date: Option<String>,
}

/// Build a lopdf `Dictionary` from a `Metadata`, setting only present keys.
pub(crate) fn build_info_dict(meta: &Metadata) -> Dictionary {
    let mut dict = Dictionary::new();
    let pairs: &[(&[u8], Option<&String>)] = &[
        (b"Title", meta.title.as_ref()),
        (b"Author", meta.author.as_ref()),
        (b"Subject", meta.subject.as_ref()),
        (b"Keywords", meta.keywords.as_ref()),
        (b"Creator", meta.creator.as_ref()),
        (b"Producer", meta.producer.as_ref()),
        (b"CreationDate", meta.creation_date.as_ref()),
        (b"ModDate", meta.mod_date.as_ref()),
    ];
    for (key, val) in pairs {
        if let Some(v) = val {
            let obj = if v.is_ascii() {
                Object::string_literal(v.as_bytes().to_vec())
            } else {
                // UTF-16BE with BOM (FE FF) for non-ASCII strings per PDF spec.
                let mut b = vec![0xFE_u8, 0xFF_u8];
                for unit in v.encode_utf16() {
                    b.extend_from_slice(&unit.to_be_bytes());
                }
                Object::String(b, StringFormat::Hexadecimal)
            };
            dict.set(key.to_vec(), obj);
        }
    }
    dict
}

/// Helper: extract a String value from a PDF string Object.
/// Handles UTF-16BE (with BOM) and PDFDocEncoding via decode_text_string.
fn get_str(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|o| decode_text_string(o).ok())
}

/// Read the Info dictionary of `data` and return it as a JSON object string.
pub fn read_metadata_json(data: &[u8]) -> Result<String, String> {
    let doc = crate::doc_io::load_pdf(data)?;

    let info_dict: Option<Dictionary> = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok().cloned(),
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        _ => None,
    };

    let meta = if let Some(d) = info_dict {
        Metadata {
            title: get_str(&d, b"Title"),
            author: get_str(&d, b"Author"),
            subject: get_str(&d, b"Subject"),
            keywords: get_str(&d, b"Keywords"),
            creator: get_str(&d, b"Creator"),
            producer: get_str(&d, b"Producer"),
            creation_date: get_str(&d, b"CreationDate"),
            mod_date: get_str(&d, b"ModDate"),
        }
    } else {
        Metadata::default()
    };

    serde_json::to_string(&meta).map_err(|e| e.to_string())
}

/// Apply metadata changes from a JSON string to `data` and return new PDF bytes
/// (incremental update). Unspecified keys from the existing Info dict are preserved.
pub fn set_metadata_json(data: &[u8], meta_json: &str, compress: bool) -> Result<Vec<u8>, String> {
    let meta: Metadata =
        serde_json::from_str(meta_json).map_err(|e| format!("invalid metadata json: {e}"))?;

    let doc = crate::doc_io::load_pdf(data)?;
    let existing_info = read_existing_info(&doc);

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    metadata_apply(&mut inc, existing_info, &meta);

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Phase A: clone any existing Info dict so unspecified keys survive the merge.
pub(crate) fn read_existing_info(doc: &Document) -> Dictionary {
    match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok().cloned().unwrap_or_default(),
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    }
}

/// Phase B: merge `meta` over `existing_info` and wire the Info dict into the
/// incremental document's trailer.
pub(crate) fn metadata_apply(
    inc: &mut IncrementalDocument,
    existing_info: Dictionary,
    meta: &Metadata,
) {
    let mut merged = existing_info;
    let overlay = build_info_dict(meta);
    for (key, val) in overlay.iter() {
        merged.set(key.clone(), val.clone());
    }

    let info_id = inc.new_document.add_object(Object::Dictionary(merged));
    inc.new_document
        .trailer
        .set("Info", Object::Reference(info_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn read_metadata_returns_json_object() {
        let json = read_metadata_json(FICHA).unwrap();
        assert!(json.starts_with('{') && json.ends_with('}'));
    }

    #[test]
    fn set_then_read_round_trips() {
        let out = set_metadata_json(
            FICHA,
            r#"{"title":"Quarterly Report","author":"ACME"}"#,
            false,
        )
        .unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental: original preserved
        let json = read_metadata_json(&out).unwrap();
        assert!(json.contains("Quarterly Report"), "json was {json}");
        assert!(json.contains("ACME"), "json was {json}");
    }

    #[test]
    fn non_ascii_metadata_round_trips() {
        let out = set_metadata_json(
            FICHA,
            r#"{"title":"日本語のタイトル","author":"Renée"}"#,
            false,
        )
        .unwrap();
        let json = read_metadata_json(&out).unwrap();
        assert!(json.contains("日本語のタイトル"), "json: {json}");
        assert!(json.contains("Renée"), "json: {json}");
    }

    #[test]
    fn ascii_metadata_still_round_trips() {
        let out = set_metadata_json(FICHA, r#"{"title":"Plain ASCII"}"#, false).unwrap();
        assert!(read_metadata_json(&out).unwrap().contains("Plain ASCII"));
    }

    #[test]
    fn reads_exotic_metadata_strings() {
        const JUST_METADATA: &[u8] =
            include_bytes!("../../../tests/fixtures/pdf-lib/just_metadata.pdf");
        let json = read_metadata_json(JUST_METADATA).unwrap();
        assert!(
            json.contains("some weird chars ˘•€"),
            "title should contain PDFDocEncoding chars; got: {json}"
        );
        assert!(
            json.contains("你怎么敢"),
            "author/subject should contain UTF-16BE Chinese; got: {json}"
        );
    }
}
