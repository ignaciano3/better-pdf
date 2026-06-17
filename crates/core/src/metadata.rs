//! Metadata module: read and incrementally write the PDF Info dictionary.

use lopdf::{Dictionary, Document, IncrementalDocument, Object};
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
            dict.set(key.to_vec(), Object::string_literal(v.as_bytes().to_vec()));
        }
    }
    dict
}

/// Helper: extract a String value from a PDF string Object (lossy UTF-8).
fn get_str(dict: &Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key) {
        Ok(Object::String(bytes, _)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

/// Read the Info dictionary of `data` and return it as a JSON object string.
pub fn read_metadata_json(data: &[u8]) -> Result<String, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;

    let info_dict: Option<Dictionary> = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => {
            doc.get_dictionary(*id).ok().cloned()
        }
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
pub fn set_metadata_json(data: &[u8], meta_json: &str) -> Result<Vec<u8>, String> {
    let meta: Metadata = serde_json::from_str(meta_json).map_err(|e| format!("invalid metadata json: {e}"))?;

    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;

    // Clone any existing Info dict so unspecified keys survive.
    let existing_info: Dictionary = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => {
            doc.get_dictionary(*id).ok().cloned().unwrap_or_default()
        }
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Build merged Info dict: start with existing, overlay provided keys.
    let mut merged = existing_info;
    let overlay = build_info_dict(&meta);
    for (key, val) in overlay.iter() {
        merged.set(key.clone(), val.clone());
    }

    // Add the Info object to new_document and wire up the trailer reference.
    let info_id = inc.new_document.add_object(Object::Dictionary(merged));
    inc.new_document.trailer.set("Info", Object::Reference(info_id));

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
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
        let out = set_metadata_json(FICHA, r#"{"title":"Quarterly Report","author":"ACME"}"#).unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental: original preserved
        let json = read_metadata_json(&out).unwrap();
        assert!(json.contains("Quarterly Report"), "json was {json}");
        assert!(json.contains("ACME"), "json was {json}");
    }
}
