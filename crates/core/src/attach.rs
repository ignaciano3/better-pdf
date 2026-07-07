//! File attachments: /EmbeddedFiles name tree write + read, /AF (associated
//! files) for ZUGFeRD/Factur-X. Same Phase A/Phase B shape as fill/flatten.

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use md5::{Digest, Md5};
use serde::Deserialize;
use std::io::Write as _;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttachOp {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub creation_date: Option<String>,
    #[serde(default)]
    pub modification_date: Option<String>,
    #[serde(default)]
    pub af_relationship: Option<String>,
    pub offset: usize,
    pub length: usize,
}

pub(crate) struct AttachPlan {
    pub root_id: ObjectId,
    /// Existing /EmbeddedFiles entries: (name, filespec object — usually a
    /// Reference) in encounter order. Empty when the doc has no tree yet.
    pub existing: Vec<(String, Object)>,
}

/// ASCII-safe fallback for /F: printable ASCII kept, everything else `_`.
fn ascii_fallback(name: &str) -> Vec<u8> {
    name.chars()
        .map(|c| if c.is_ascii() && !c.is_ascii_control() { c as u8 } else { b'_' })
        .collect()
}

/// UTF-16BE with BOM, the PDF text-string encoding for non-ASCII names.
fn utf16be_string(s: &str) -> Vec<u8> {
    let mut out = vec![0xFE, 0xFF];
    for unit in s.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

pub(crate) fn attach_resolve(
    doc: &Document,
    ops: &[AttachOp],
    blob: &[u8],
) -> Result<AttachPlan, String> {
    // Validate blob ranges up front so apply can slice unchecked.
    for op in ops {
        op.offset
            .checked_add(op.length)
            .filter(|&e| e <= blob.len())
            .ok_or_else(|| {
                format!(
                    "attachment '{}' byte range {}..{} out of range (blob is {} bytes)",
                    op.name,
                    op.offset,
                    op.offset.saturating_add(op.length),
                    blob.len()
                )
            })?;
    }
    // Duplicates within the queued ops themselves.
    let mut seen = std::collections::HashSet::new();
    for op in ops {
        if !seen.insert(op.name.as_str()) {
            return Err(format!("duplicate attachment name '{}'", op.name));
        }
    }
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    // Task 2 replaces this with a real walk of any existing tree.
    Ok(AttachPlan { root_id, existing: Vec::new() })
}

/// Build the /EmbeddedFile stream + /Filespec dict for one op; returns the
/// filespec's object id.
fn build_filespec(
    new_doc: &mut Document,
    op: &AttachOp,
    blob: &[u8],
) -> Result<ObjectId, String> {
    let bytes = &blob[op.offset..op.offset + op.length];

    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).map_err(|e| e.to_string())?;
    let compressed = enc.finish().map_err(|e| e.to_string())?;

    let mut params = Dictionary::new();
    params.set("Size", Object::Integer(bytes.len() as i64));
    let checksum: [u8; 16] = Md5::digest(bytes).into();
    params.set(
        "CheckSum",
        Object::String(checksum.to_vec(), lopdf::StringFormat::Hexadecimal),
    );
    if let Some(d) = &op.creation_date {
        params.set(
            "CreationDate",
            Object::String(d.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(d) = &op.modification_date {
        params.set(
            "ModDate",
            Object::String(d.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }

    let mut sdict = Dictionary::new();
    sdict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
    if let Some(mime) = &op.mime_type {
        // lopdf's writer #-escapes delimiter chars in names (e.g. '/' →
        // "#2F"), so the raw MIME bytes are correct here.
        sdict.set("Subtype", Object::Name(mime.as_bytes().to_vec()));
    }
    sdict.set("Params", Object::Dictionary(params));
    sdict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    let mut stream = Stream::new(sdict, compressed);
    // The content is already compressed; prevent lopdf/compress passes from
    // touching it.
    stream.dict.set("Length", Object::Integer(stream.content.len() as i64));
    let stream_id = new_doc.add_object(Object::Stream(stream));

    let mut ef = Dictionary::new();
    ef.set("F", Object::Reference(stream_id));
    ef.set("UF", Object::Reference(stream_id));

    let mut spec = Dictionary::new();
    spec.set("Type", Object::Name(b"Filespec".to_vec()));
    spec.set(
        "F",
        Object::String(ascii_fallback(&op.name), lopdf::StringFormat::Literal),
    );
    spec.set(
        "UF",
        Object::String(utf16be_string(&op.name), lopdf::StringFormat::Hexadecimal),
    );
    if let Some(desc) = &op.description {
        spec.set(
            "Desc",
            Object::String(desc.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(rel) = &op.af_relationship {
        spec.set("AFRelationship", Object::Name(rel.as_bytes().to_vec()));
    }
    spec.set("EF", Object::Dictionary(ef));
    Ok(new_doc.add_object(Object::Dictionary(spec)))
}

pub(crate) fn attach_apply(
    inc: &mut IncrementalDocument,
    plan: &AttachPlan,
    ops: &[AttachOp],
    blob: &[u8],
) -> Result<(), String> {
    if ops.is_empty() {
        return Ok(());
    }

    // Build the new filespecs.
    let mut entries: Vec<(String, Object)> = plan.existing.clone();
    for op in ops {
        let spec_id = build_filespec(&mut inc.new_document, op, blob)?;
        entries.push((op.name.clone(), Object::Reference(spec_id)));
    }
    // Name trees must be sorted (byte order of the name strings).
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut flat = Vec::with_capacity(entries.len() * 2);
    for (name, spec) in &entries {
        flat.push(Object::String(
            name.as_bytes().to_vec(),
            lopdf::StringFormat::Literal,
        ));
        flat.push(spec.clone());
    }
    let mut ef_node = Dictionary::new();
    ef_node.set("Names", Object::Array(flat));
    let ef_id = inc.new_document.add_object(Object::Dictionary(ef_node));

    // Override the catalog (same-object-id incremental override; the pattern
    // outline_apply uses). Merge into any existing /Names dict rather than
    // clobbering other name trees (e.g. /Dests, /JavaScript).
    inc.opt_clone_object_to_new_document(plan.root_id)
        .map_err(|e| e.to_string())?;
    // Read the existing /Names value BEFORE taking the mutable catalog borrow.
    let existing_names: Option<Object> = inc
        .new_document
        .get_dictionary(plan.root_id)
        .ok()
        .and_then(|c| c.get(b"Names").ok().cloned());
    let mut names_dict = match existing_names {
        Some(Object::Dictionary(d)) => d,
        Some(Object::Reference(id)) => inc
            .new_document
            .get_dictionary(id)
            .or_else(|_| {
                // /Names lives in a prior revision: resolve through prev docs.
                inc.get_prev_documents().get_dictionary(id)
            })
            .map_err(|e| e.to_string())?
            .clone(),
        _ => Dictionary::new(),
    };
    names_dict.set("EmbeddedFiles", Object::Reference(ef_id));

    let catalog = inc
        .new_document
        .get_object_mut(plan.root_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?;
    catalog.set("Names", Object::Dictionary(names_dict));
    Ok(())
}

/// Standalone entry: parse ops, load doc, resolve, apply, save incrementally.
pub fn attach_files_json(
    data: &[u8],
    ops_json: &str,
    blob: &[u8],
    compress: bool,
) -> Result<Vec<u8>, String> {
    let ops: Vec<AttachOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid attach ops: {e}"))?;
    let doc = crate::doc_io::load_pdf(data)?;
    let plan = attach_resolve(&doc, &ops, blob)?;
    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    attach_apply(&mut inc, &plan, &ops, blob)?;
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

    fn blank_doc() -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[], &[], "[]", "[]", false, false,
        )
        .unwrap()
    }

    /// (name, filespec dict) pairs from /Root/Names/EmbeddedFiles/Names,
    /// resolving references. Panics on malformed structure — tests only.
    fn tree_entries(doc: &Document) -> Vec<(String, Dictionary)> {
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        let names = match catalog.get(b"Names").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            o => panic!("bad /Names: {o:?}"),
        };
        let ef = match names.get(b"EmbeddedFiles").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            o => panic!("bad /EmbeddedFiles: {o:?}"),
        };
        let arr = ef.get(b"Names").unwrap().as_array().unwrap();
        arr.chunks(2)
            .map(|pair| {
                let name = String::from_utf8(pair[0].as_str().unwrap().to_vec()).unwrap();
                let spec = match &pair[1] {
                    Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
                    Object::Dictionary(d) => d.clone(),
                    o => panic!("bad filespec: {o:?}"),
                };
                (name, spec)
            })
            .collect()
    }

    /// Decompressed /EF /F stream bytes of a filespec dict.
    fn ef_bytes(doc: &Document, spec: &Dictionary) -> Vec<u8> {
        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let stream = doc.get_object(sid).unwrap().as_stream().unwrap();
        stream.decompressed_content().unwrap()
    }

    #[test]
    fn attach_creates_names_tree_and_embedded_file_stream() {
        let base = blank_doc();
        let payload = b"<invoice>42</invoice>".to_vec();
        let ops = format!(
            r#"[{{"name":"factur-x.xml","mimeType":"text/xml","description":"Invoice data","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let out = attach_files_json(&base, &ops, &payload, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();

        let entries = tree_entries(&doc);
        assert_eq!(entries.len(), 1);
        let (name, spec) = &entries[0];
        assert_eq!(name, "factur-x.xml");

        // Filespec shape
        assert_eq!(spec.get(b"Type").unwrap().as_name().unwrap(), b"Filespec");
        assert_eq!(spec.get(b"F").unwrap().as_str().unwrap(), b"factur-x.xml");
        // /UF is UTF-16BE with BOM
        let uf = spec.get(b"UF").unwrap().as_str().unwrap();
        assert_eq!(&uf[..2], &[0xFE, 0xFF]);
        assert_eq!(
            spec.get(b"Desc").unwrap().as_str().unwrap(),
            b"Invoice data"
        );

        // Stream: decompresses to payload, FlateDecode, Subtype, Params
        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let stream = doc.get_object(sid).unwrap().as_stream().unwrap();
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert_eq!(
            stream.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"text/xml"
        );
        assert_eq!(ef_bytes(&doc, spec), payload);

        let params = stream.dict.get(b"Params").unwrap().as_dict().unwrap();
        assert_eq!(
            params.get(b"Size").unwrap().as_i64().unwrap(),
            payload.len() as i64
        );
        let expected_md5: [u8; 16] = Md5::digest(&payload).into();
        assert_eq!(params.get(b"CheckSum").unwrap().as_str().unwrap(), &expected_md5);
        // No dates were passed → none written
        assert!(params.get(b"CreationDate").is_err());
        assert!(params.get(b"ModDate").is_err());
    }

    #[test]
    fn attach_writes_optional_dates_and_unicode_uf() {
        let base = blank_doc();
        let payload = b"data".to_vec();
        let ops = format!(
            r#"[{{"name":"año-2026.txt","creationDate":"D:20260101120000Z","modificationDate":"D:20260102120000Z","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let out = attach_files_json(&base, &ops, &payload, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, spec) = &tree_entries(&doc)[0];

        // /UF round-trips the ñ via UTF-16BE
        let uf = spec.get(b"UF").unwrap().as_str().unwrap();
        let utf16: Vec<u16> = uf[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&utf16).unwrap(), "año-2026.txt");
        // /F is the ASCII-safe fallback (non-ASCII replaced with '_')
        assert_eq!(spec.get(b"F").unwrap().as_str().unwrap(), b"a_o-2026.txt");

        let ef = spec.get(b"EF").unwrap().as_dict().unwrap();
        let sid = ef.get(b"F").unwrap().as_reference().unwrap();
        let params = doc
            .get_object(sid).unwrap().as_stream().unwrap()
            .dict.get(b"Params").unwrap().as_dict().unwrap();
        assert_eq!(
            params.get(b"CreationDate").unwrap().as_str().unwrap(),
            b"D:20260101120000Z"
        );
        assert_eq!(
            params.get(b"ModDate").unwrap().as_str().unwrap(),
            b"D:20260102120000Z"
        );
    }

    #[test]
    fn attach_two_files_sorted_lexicographically() {
        let base = blank_doc();
        let blob = b"AABB".to_vec();
        // Queued out of order: "b.txt" first, "a.txt" second.
        let ops = r#"[
            {"name":"b.txt","offset":0,"length":2},
            {"name":"a.txt","offset":2,"length":2}
        ]"#;
        let out = attach_files_json(&base, ops, &blob, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let entries = tree_entries(&doc);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
        assert_eq!(ef_bytes(&doc, &entries[0].1), b"BB");
        assert_eq!(ef_bytes(&doc, &entries[1].1), b"AA");
    }

    #[test]
    fn duplicate_queued_names_error() {
        let base = blank_doc();
        let blob = b"xxyy".to_vec();
        let ops = r#"[
            {"name":"same.txt","offset":0,"length":2},
            {"name":"same.txt","offset":2,"length":2}
        ]"#;
        let err = attach_files_json(&base, ops, &blob, false).unwrap_err();
        assert!(
            err.starts_with("duplicate attachment"),
            "error must start with the stable prefix: {err}"
        );
        assert!(err.contains("same.txt"));
    }

    #[test]
    fn attach_blob_range_out_of_bounds_errors() {
        let base = blank_doc();
        let ops = r#"[{"name":"a.txt","offset":0,"length":99}]"#;
        let err = attach_files_json(&base, ops, b"tiny", false).unwrap_err();
        assert!(err.contains("out of range"), "unexpected: {err}");
    }

    #[test]
    fn attach_offset_overflow_errors_without_panic() {
        // offset + length overflows usize: must be a clean Err, not a panic.
        let base = blank_doc();
        let ops = r#"[{"name":"a.txt","offset":18446744073709551615,"length":1}]"#;
        let err = attach_files_json(&base, ops, b"tiny", false).unwrap_err();
        assert!(err.contains("out of range"), "unexpected: {err}");
    }

    #[test]
    fn attached_output_is_incremental_append() {
        // Incremental save must preserve the original bytes as a prefix.
        let base = blank_doc();
        let ops = r#"[{"name":"a.txt","offset":0,"length":4}]"#;
        let out = attach_files_json(&base, ops, b"data", false).unwrap();
        assert!(out.len() > base.len());
        assert_eq!(&out[..base.len()], &base[..]);
    }
}
