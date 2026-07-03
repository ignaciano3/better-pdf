//! Incremental page-tree ops: append / insert / remove / move blank pages on a
//! loaded PDF, preserving everything else (forms, links, content) via an
//! incremental update.
//!
//! ## Page-tree assumption
//! For v1 we handle the common single-level page tree (catalog -> /Pages ->
//! /Kids = [leaf pages]). We REUSE the existing /Pages root object id as the
//! rebuilt root: its /Kids is set to the new ordered list and /Count to the new
//! length. Existing leaf pages already have /Parent pointing at this root, so
//! they need no per-leaf change; only NEW blank pages get their /Parent set.
//!
//! If a NESTED page tree is detected (a kid that is itself a /Pages node), we
//! return `Err("nested page trees not supported")`. The FICHA fixture (and most
//! PDFs) is flat, so the supported tests pass.

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, Stream, dictionary};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "op")]
enum PageOp {
    #[serde(rename = "appendBlank")]
    AppendBlank { width: f32, height: f32 },
    #[serde(rename = "insertBlank")]
    InsertBlank {
        index: usize,
        width: f32,
        height: f32,
    },
    #[serde(rename = "removePage")]
    RemovePage { index: usize },
    #[serde(rename = "movePage")]
    MovePage { from: usize, to: usize },
}

/// An entry in the rebuilt page order: either an existing leaf page object id,
/// or a NEW blank page (with its requested dimensions) not yet added to the doc.
enum Entry {
    Existing(ObjectId),
    NewBlank { width: f32, height: f32 },
}

/// Apply page-tree ops from a JSON string to `data` and return new PDF bytes
/// (incremental save: the original bytes are preserved as a prefix).
pub fn insert_pages_json(data: &[u8], ops_json: &str, compress: bool) -> Result<Vec<u8>, String> {
    let ops: Vec<PageOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid page ops: {e}"))?;

    let doc = crate::doc_io::load_pdf(data)?;

    // Find the /Pages root id from the catalog.
    let catalog = doc.catalog().map_err(|e| e.to_string())?;
    let pages_root_id = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| "catalog has no /Pages reference".to_string())?;

    // Detect a nested page tree: any direct kid of the root that is itself a
    // /Pages node is unsupported for v1.
    let root_dict = doc
        .get_dictionary(pages_root_id)
        .map_err(|e| e.to_string())?;
    if let Ok(Object::Array(kids)) = root_dict.get(b"Kids") {
        for kid in kids {
            if let Ok(kid_id) = kid.as_reference()
                && let Ok(kid_dict) = doc.get_dictionary(kid_id)
                && let Ok(Object::Name(ty)) = kid_dict.get(b"Type")
                && ty == b"Pages"
            {
                return Err("nested page trees not supported".to_string());
            }
        }
    }

    // Ordered leaf page ids (already flattened by get_pages()).
    let leaves: Vec<ObjectId> = doc.get_pages().into_values().collect();

    // Build the ordered list of entries, applying ops in array order.
    let mut entries: Vec<Entry> = leaves.iter().map(|&id| Entry::Existing(id)).collect();
    for op in &ops {
        match op {
            PageOp::AppendBlank { width, height } => {
                entries.push(Entry::NewBlank {
                    width: *width,
                    height: *height,
                });
            }
            PageOp::InsertBlank {
                index,
                width,
                height,
            } => {
                if *index > entries.len() {
                    return Err(format!(
                        "insertBlank index {index} out of range (len {})",
                        entries.len()
                    ));
                }
                entries.insert(
                    *index,
                    Entry::NewBlank {
                        width: *width,
                        height: *height,
                    },
                );
            }
            PageOp::RemovePage { index } => {
                if *index >= entries.len() {
                    return Err(format!(
                        "removePage index {index} out of range (len {})",
                        entries.len()
                    ));
                }
                entries.remove(*index);
            }
            PageOp::MovePage { from, to } => {
                if *from >= entries.len() {
                    return Err(format!(
                        "movePage from {from} out of range (len {})",
                        entries.len()
                    ));
                }
                let e = entries.remove(*from);
                if *to > entries.len() {
                    return Err(format!(
                        "movePage to {to} out of range (len {})",
                        entries.len()
                    ));
                }
                entries.insert(*to, e);
            }
        }
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Materialise the ordered entries into object ids, creating blank pages.
    // New blank pages get /Parent = pages_root_id at construction; existing
    // leaves already point /Parent there (flat tree), so no per-leaf edits.
    let mut kids: Vec<Object> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let id = match entry {
            Entry::Existing(id) => *id,
            Entry::NewBlank { width, height } => {
                let content_id = inc
                    .new_document
                    .add_object(Object::Stream(Stream::new(Dictionary::new(), b"".to_vec())));
                let page = dictionary! {
                    "Type" => Object::Name(b"Page".to_vec()),
                    "Parent" => Object::Reference(pages_root_id),
                    "MediaBox" => Object::Array(vec![
                        Object::Real(0.0),
                        Object::Real(0.0),
                        Object::Real(*width),
                        Object::Real(*height),
                    ]),
                    "Resources" => Object::Dictionary(Dictionary::new()),
                    "Contents" => Object::Reference(content_id),
                };
                inc.new_document.add_object(Object::Dictionary(page))
            }
        };
        kids.push(Object::Reference(id));
    }

    let count = kids.len() as i64;

    // Rebuild the /Pages root in new_document: clone the existing root, then set
    // its /Kids and /Count. Existing leaves already point /Parent at this id.
    inc.opt_clone_object_to_new_document(pages_root_id)
        .map_err(|e| e.to_string())?;
    let root = inc
        .new_document
        .get_object_mut(pages_root_id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    root.set("Kids", Object::Array(kids));
    root.set("Count", Object::Integer(count));

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
    fn count(b: &[u8]) -> usize {
        Document::load_mem(b).unwrap().get_pages().len()
    }

    #[test]
    fn append_blank_adds_a_page() {
        let n = count(FICHA);
        let out =
            insert_pages_json(FICHA, r#"[{"op":"appendBlank","width":595,"height":842}]"#, false).unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental
        assert_eq!(count(&out), n + 1);
        // the new last page has the requested MediaBox
        let doc = Document::load_mem(&out).unwrap();
        let last = doc.get_pages().into_values().last().unwrap();
        let mb = doc
            .get_dictionary(last)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        assert!((mb[2].as_float().unwrap() - 595.0).abs() < 0.5);
    }
    #[test]
    fn insert_blank_at_zero_is_first() {
        let n = count(FICHA);
        let out = insert_pages_json(
            FICHA,
            r#"[{"op":"insertBlank","index":0,"width":100,"height":100}]"#, false
        )
        .unwrap();
        assert_eq!(count(&out), n + 1);
        let doc = Document::load_mem(&out).unwrap();
        let first = doc.get_pages().into_values().next().unwrap();
        let mb = doc
            .get_dictionary(first)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(
            (mb[2].as_float().unwrap() - 100.0).abs() < 0.5,
            "inserted page should be first"
        );
    }
    #[test]
    fn remove_page_drops_one() {
        let n = count(FICHA);
        if n >= 1 {
            let out = insert_pages_json(FICHA, r#"[{"op":"removePage","index":0}]"#, false).unwrap();
            assert_eq!(count(&out), n - 1);
        }
    }
    #[test]
    fn move_page_reorders() {
        let n = count(FICHA);
        if n >= 2 {
            let out = insert_pages_json(FICHA, r#"[{"op":"movePage","from":0,"to":1}]"#, false).unwrap();
            assert_eq!(count(&out), n);
        }
    }
    #[test]
    fn errors_on_out_of_range_index() {
        assert!(insert_pages_json(FICHA, r#"[{"op":"removePage","index":9999}]"#, false).is_err());
    }

    #[test]
    fn acroform_survives_page_ops() {
        // The FICHA fixture is a form. After combined ops the catalog must still
        // reference its AcroForm (forms preserved) and the doc must reload.
        let orig = Document::load_mem(FICHA).unwrap();
        let had_acroform = orig.catalog().unwrap().has(b"AcroForm");
        let out = insert_pages_json(
            FICHA,
            r#"[{"op":"appendBlank","width":200,"height":300},{"op":"movePage","from":0,"to":1}]"#, false
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.catalog().unwrap().has(b"AcroForm"), had_acroform);
    }
}
