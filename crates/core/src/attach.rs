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

/// Decode a PDF text string: UTF-16BE with BOM, or bytes as Latin-1/UTF-8.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&utf16);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Recursively collect (name, filespec object) pairs from a name-tree node
/// (either a leaf with /Names or an interior node with /Kids).
fn walk_name_tree(
    doc: &Document,
    node: &Dictionary,
    out: &mut Vec<(String, Object)>,
) -> Result<(), String> {
    walk_name_tree_inner(doc, node, out, 0)
}

/// A conforming name tree is shallow; the cap turns crafted /Kids reference
/// cycles (infinitely deep) into an error instead of unbounded recursion.
const MAX_NAME_TREE_DEPTH: u32 = 64;

fn walk_name_tree_inner(
    doc: &Document,
    node: &Dictionary,
    out: &mut Vec<(String, Object)>,
    depth: u32,
) -> Result<(), String> {
    if depth > MAX_NAME_TREE_DEPTH {
        return Err("embedded-files name tree exceeds maximum depth (cyclic /Kids?)".into());
    }
    if let Ok(kids) = node.get(b"Kids").and_then(|o| o.as_array()) {
        for kid in kids {
            let kid_dict = match kid {
                Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string())?,
                Object::Dictionary(d) => d,
                other => return Err(format!("malformed name-tree kid: {other:?}")),
            };
            walk_name_tree_inner(doc, kid_dict, out, depth + 1)?;
        }
    }
    if let Ok(pairs) = node.get(b"Names").and_then(|o| o.as_array()) {
        for pair in pairs.chunks(2) {
            if pair.len() != 2 {
                continue;
            }
            let name = pair[0]
                .as_str()
                .map(decode_pdf_string)
                .map_err(|e| e.to_string())?;
            out.push((name, pair[1].clone()));
        }
    }
    Ok(())
}

/// Resolve a dict-or-reference object to a Dictionary in `doc`.
fn resolve_dict<'a>(doc: &'a Document, obj: &'a Object) -> Result<&'a Dictionary, String> {
    match obj {
        Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string()),
        Object::Dictionary(d) => Ok(d),
        other => Err(format!("expected dictionary, got {other:?}")),
    }
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
            // Envelope detail is the bare name; TS reconstructs the message.
            return Err(crate::coded_error(
                crate::error_code::DUPLICATE_ATTACHMENT,
                op.name.clone(),
            ));
        }
    }
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;

    let mut existing = Vec::new();
    if let Ok(catalog) = doc.get_dictionary(root_id)
        && let Ok(names_obj) = catalog.get(b"Names")
    {
        let names = resolve_dict(doc, names_obj)?;
        if let Ok(ef_obj) = names.get(b"EmbeddedFiles") {
            let ef = resolve_dict(doc, ef_obj)?;
            walk_name_tree(doc, ef, &mut existing)?;
        }
    }
    // The existing names use /UF-preferred strings already? No — name-tree
    // KEYS are the canonical names (the /UF preference applies to reading
    // filespec metadata, Task 3). Compare queued names against the tree keys.
    for op in ops {
        if existing.iter().any(|(n, _)| n == &op.name) {
            // Envelope detail is the bare name; TS reconstructs the message.
            return Err(crate::coded_error(
                crate::error_code::DUPLICATE_ATTACHMENT,
                op.name.clone(),
            ));
        }
    }
    Ok(AttachPlan { root_id, existing })
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
    let mut built: Vec<(&AttachOp, ObjectId)> = Vec::with_capacity(ops.len());
    for op in ops {
        let spec_id = build_filespec(&mut inc.new_document, op, blob)?;
        entries.push((op.name.clone(), Object::Reference(spec_id)));
        built.push((op, spec_id));
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

    // /AF: filespec refs of every op that declared an afRelationship.
    let af_new: Vec<Object> = built
        .iter()
        .filter(|(op, _)| op.af_relationship.is_some())
        .map(|(_, id)| Object::Reference(*id))
        .collect();
    if !af_new.is_empty() {
        // Existing /AF read from the (possibly just-cloned) catalog.
        let mut af = match inc
            .new_document
            .get_dictionary(plan.root_id)
            .ok()
            .and_then(|c| c.get(b"AF").ok())
        {
            Some(Object::Array(a)) => a.clone(),
            Some(Object::Reference(id)) => {
                let id = *id;
                inc.new_document
                    .get_object(id)
                    .or_else(|_| inc.get_prev_documents().get_object(id))
                    .ok()
                    .and_then(|o| o.as_array().ok().cloned())
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        af.extend(af_new);
        let catalog = inc
            .new_document
            .get_object_mut(plan.root_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        catalog.set("AF", Object::Array(af));
    }
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadAttachment {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    creation_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modification_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    af_relationship: Option<String>,
    size: usize,
    offset: usize,
    length: usize,
}

fn dict_string(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok()?.as_str().ok().map(decode_pdf_string)
}

/// Walk /Names/EmbeddedFiles and return `[u32 LE json_len][json][bytes blob]`.
/// Filespecs without a decodable /EF stream are skipped, not fatal.
pub fn read_attachments_packed(data: &[u8]) -> Result<Vec<u8>, String> {
    let doc = crate::doc_io::load_pdf(data)?;
    let mut entries = Vec::new();
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    if let Ok(catalog) = doc.get_dictionary(root_id)
        && let Ok(names_obj) = catalog.get(b"Names")
        && let Ok(names) = resolve_dict(&doc, names_obj)
        && let Ok(ef_obj) = names.get(b"EmbeddedFiles")
        && let Ok(ef) = resolve_dict(&doc, ef_obj)
    {
        walk_name_tree(&doc, ef, &mut entries)?;
    }

    let mut metas = Vec::new();
    let mut blob = Vec::new();
    for (tree_name, spec_obj) in &entries {
        let Ok(spec) = resolve_dict(&doc, spec_obj) else { continue };
        // /EF /F preferred, /UF fallback.
        let Ok(ef) = spec.get(b"EF").and_then(|o| o.as_dict()) else { continue };
        let stream_ref = ef.get(b"F").or_else(|_| ef.get(b"UF"));
        let Ok(stream_id) = stream_ref.and_then(|o| o.as_reference()) else { continue };
        let Ok(stream) = doc.get_object(stream_id).and_then(|o| o.as_stream()) else { continue };
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());

        // Name: filespec /UF preferred, then /F, then the tree key.
        let name = dict_string(spec, b"UF")
            .or_else(|| dict_string(spec, b"F"))
            .unwrap_or_else(|| tree_name.clone());
        let params = stream.dict.get(b"Params").and_then(|o| o.as_dict()).ok();

        let offset = blob.len();
        let length = bytes.len();
        blob.extend_from_slice(&bytes);
        metas.push(ReadAttachment {
            name,
            description: dict_string(spec, b"Desc"),
            mime_type: stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned()),
            creation_date: params.and_then(|p| dict_string(p, b"CreationDate")),
            modification_date: params.and_then(|p| dict_string(p, b"ModDate")),
            af_relationship: spec
                .get(b"AFRelationship")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned()),
            size: length,
            offset,
            length,
        });
    }

    let json = serde_json::to_vec(&metas).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(4 + json.len() + blob.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&blob);
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
            crate::err_has_code(&err, crate::error_code::DUPLICATE_ATTACHMENT),
            "error must carry the duplicate-attachment code: {err}"
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

    /// A doc with an existing /EmbeddedFiles tree split into two /Kids leaf
    /// nodes: ["alpha.txt"] and ["zeta.txt"], each with /Limits. Built by
    /// attaching nothing — we construct the objects directly on a blank doc
    /// and save it non-incrementally via lopdf.
    fn doc_with_kids_tree() -> Vec<u8> {
        let base = blank_doc();
        let mut doc = Document::load_mem(&base).unwrap();

        let mk_spec = |doc: &mut Document, name: &str, content: &[u8]| -> ObjectId {
            let mut sdict = Dictionary::new();
            sdict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
            let stream_id = doc.add_object(Object::Stream(Stream::new(sdict, content.to_vec())));
            let mut ef = Dictionary::new();
            ef.set("F", Object::Reference(stream_id));
            let mut spec = Dictionary::new();
            spec.set("Type", Object::Name(b"Filespec".to_vec()));
            spec.set("F", Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal));
            spec.set("EF", Object::Dictionary(ef));
            doc.add_object(Object::Dictionary(spec))
        };
        let alpha = mk_spec(&mut doc, "alpha.txt", b"ALPHA");
        let zeta = mk_spec(&mut doc, "zeta.txt", b"ZETA");

        let leaf = |doc: &mut Document, name: &str, spec: ObjectId| -> ObjectId {
            let mut d = Dictionary::new();
            d.set("Limits", Object::Array(vec![
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            ]));
            d.set("Names", Object::Array(vec![
                Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                Object::Reference(spec),
            ]));
            doc.add_object(Object::Dictionary(d))
        };
        let k1 = leaf(&mut doc, "alpha.txt", alpha);
        let k2 = leaf(&mut doc, "zeta.txt", zeta);

        let mut ef_root = Dictionary::new();
        ef_root.set("Kids", Object::Array(vec![Object::Reference(k1), Object::Reference(k2)]));
        let ef_root_id = doc.add_object(Object::Dictionary(ef_root));
        let mut names = Dictionary::new();
        names.set("EmbeddedFiles", Object::Reference(ef_root_id));

        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_object_mut(root_id).unwrap().as_dict_mut().unwrap();
        catalog.set("Names", Object::Dictionary(names));

        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn merge_preserves_existing_kids_tree_entries_in_sorted_order() {
        let base = doc_with_kids_tree();
        let ops = r#"[{"name":"beta.txt","offset":0,"length":4}]"#;
        let out = attach_files_json(&base, ops, b"BETA", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let entries = tree_entries(&doc);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        // Existing alpha/zeta preserved, beta merged in sorted position,
        // flat root node (tree_entries reads /Names directly — no /Kids).
        assert_eq!(names, vec!["alpha.txt", "beta.txt", "zeta.txt"]);
        assert_eq!(ef_bytes(&doc, &entries[0].1), b"ALPHA");
        assert_eq!(ef_bytes(&doc, &entries[1].1), b"BETA");
        assert_eq!(ef_bytes(&doc, &entries[2].1), b"ZETA");
    }

    #[test]
    fn duplicate_against_existing_tree_errors() {
        let base = doc_with_kids_tree();
        let ops = r#"[{"name":"alpha.txt","offset":0,"length":3}]"#;
        let err = attach_files_json(&base, ops, b"NEW", false).unwrap_err();
        assert!(
            crate::err_has_code(&err, crate::error_code::DUPLICATE_ATTACHMENT),
            "{err}"
        );
        // Envelope detail is the bare name.
        assert!(err.ends_with(":alpha.txt"), "{err}");
    }

    #[test]
    fn af_relationship_sets_filespec_key_and_catalog_af() {
        let base = blank_doc();
        let ops = r#"[
            {"name":"factur-x.xml","afRelationship":"Alternative","offset":0,"length":3},
            {"name":"other.txt","offset":3,"length":3}
        ]"#;
        let out = attach_files_json(&base, ops, b"XMLTXT", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();

        let entries = tree_entries(&doc);
        let facturx = &entries.iter().find(|(n, _)| n == "factur-x.xml").unwrap().1;
        assert_eq!(
            facturx.get(b"AFRelationship").unwrap().as_name().unwrap(),
            b"Alternative"
        );
        // other.txt has no /AFRelationship
        let other = &entries.iter().find(|(n, _)| n == "other.txt").unwrap().1;
        assert!(other.get(b"AFRelationship").is_err());

        // Catalog /AF holds exactly the factur-x filespec ref.
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        let af = catalog.get(b"AF").unwrap().as_array().unwrap();
        assert_eq!(af.len(), 1);
        let af_spec = doc
            .get_dictionary(af[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(af_spec.get(b"F").unwrap().as_str().unwrap(), b"factur-x.xml");
    }

    #[test]
    fn af_array_appends_preserving_existing_entries() {
        let base = blank_doc();
        let first = attach_files_json(
            &base,
            r#"[{"name":"a.xml","afRelationship":"Data","offset":0,"length":1}]"#,
            b"A", false,
        )
        .unwrap();
        let out = attach_files_json(
            &first,
            r#"[{"name":"b.xml","afRelationship":"Source","offset":0,"length":1}]"#,
            b"B", false,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let af = doc
            .get_dictionary(root_id).unwrap()
            .get(b"AF").unwrap().as_array().unwrap();
        assert_eq!(af.len(), 2, "existing /AF entry must be preserved");
    }

    #[test]
    fn second_attach_pass_merges_with_first() {
        // Two sequential standalone attaches (the chained-save scenario).
        let base = blank_doc();
        let first =
            attach_files_json(&base, r#"[{"name":"one.txt","offset":0,"length":3}]"#, b"ONE", false)
                .unwrap();
        let out =
            attach_files_json(&first, r#"[{"name":"two.txt","offset":0,"length":3}]"#, b"TWO", false)
                .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let names: Vec<String> = tree_entries(&doc).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["one.txt", "two.txt"]);
    }

    /// Decode the packed read_attachments buffer into (json, blob).
    fn unpack(packed: &[u8]) -> (serde_json::Value, Vec<u8>) {
        let json_len = u32::from_le_bytes(packed[..4].try_into().unwrap()) as usize;
        let json: serde_json::Value =
            serde_json::from_slice(&packed[4..4 + json_len]).unwrap();
        (json, packed[4 + json_len..].to_vec())
    }

    #[test]
    fn read_attachments_round_trips_metadata_and_bytes() {
        let base = blank_doc();
        let payload = b"<xml>invoice</xml>".to_vec();
        let ops = format!(
            r#"[{{"name":"año.xml","mimeType":"text/xml","description":"desc","creationDate":"D:20260101120000Z","afRelationship":"Alternative","offset":0,"length":{}}}]"#,
            payload.len()
        );
        let saved = attach_files_json(&base, &ops, &payload, false).unwrap();

        let (json, blob) = unpack(&read_attachments_packed(&saved).unwrap());
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let a = &arr[0];
        assert_eq!(a["name"], "año.xml"); // /UF preferred over the a_o.xml /F fallback
        assert_eq!(a["mimeType"], "text/xml");
        assert_eq!(a["description"], "desc");
        assert_eq!(a["creationDate"], "D:20260101120000Z");
        assert_eq!(a["afRelationship"], "Alternative");
        assert_eq!(a["size"], payload.len());
        assert!(a.get("modificationDate").is_none(), "absent key must be omitted");

        let off = a["offset"].as_u64().unwrap() as usize;
        let len = a["length"].as_u64().unwrap() as usize;
        assert_eq!(&blob[off..off + len], &payload[..]);
    }

    #[test]
    fn read_attachments_walks_kids_and_skips_specs_without_ef() {
        let base = doc_with_kids_tree(); // alpha.txt + zeta.txt (uncompressed streams)
        // Add a broken filespec (no /EF) to the tree by attaching a valid one
        // first, then hand-editing: simpler — build a doc where one leaf entry
        // is a /Filespec without /EF.
        let mut doc = Document::load_mem(&base).unwrap();
        let mut spec = Dictionary::new();
        spec.set("Type", Object::Name(b"Filespec".to_vec()));
        spec.set("F", Object::String(b"broken.txt".to_vec(), lopdf::StringFormat::Literal));
        let broken = doc.add_object(Object::Dictionary(spec));
        // splice it into the first /Kids leaf's /Names array
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let names_obj = doc.get_dictionary(root_id).unwrap().get(b"Names").unwrap().clone();
        let ef_root_id = match &names_obj {
            Object::Dictionary(d) => d.get(b"EmbeddedFiles").unwrap().as_reference().unwrap(),
            _ => panic!(),
        };
        let kid0 = doc.get_dictionary(ef_root_id).unwrap()
            .get(b"Kids").unwrap().as_array().unwrap()[0].as_reference().unwrap();
        let kid = doc.get_object_mut(kid0).unwrap().as_dict_mut().unwrap();
        let mut names = kid.get(b"Names").unwrap().as_array().unwrap().clone();
        names.push(Object::String(b"broken.txt".to_vec(), lopdf::StringFormat::Literal));
        names.push(Object::Reference(broken));
        kid.set("Names", Object::Array(names));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let (json, blob) = unpack(&read_attachments_packed(&bytes).unwrap());
        let names: Vec<&str> = json
            .as_array().unwrap().iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        // broken.txt skipped (no /EF), not fatal
        assert_eq!(names, vec!["alpha.txt", "zeta.txt"]);
        let a0 = &json[0];
        let off = a0["offset"].as_u64().unwrap() as usize;
        let len = a0["length"].as_u64().unwrap() as usize;
        assert_eq!(&blob[off..off + len], b"ALPHA"); // uncompressed stream fallback
    }

    #[test]
    fn read_attachments_empty_doc_returns_empty_array() {
        let (json, blob) = unpack(&read_attachments_packed(&blank_doc()).unwrap());
        assert_eq!(json.as_array().unwrap().len(), 0);
        assert!(blob.is_empty());
    }

    #[test]
    fn cyclic_kids_tree_errors_instead_of_overflowing() {
        // A name-tree node whose /Kids references itself: walking it must
        // hit the depth cap and error, not recurse forever.
        let base = blank_doc();
        let mut doc = Document::load_mem(&base).unwrap();

        let node_id = doc.add_object(Object::Dictionary(Dictionary::new()));
        let mut node = Dictionary::new();
        node.set("Kids", Object::Array(vec![Object::Reference(node_id)]));
        *doc.get_object_mut(node_id).unwrap() = Object::Dictionary(node);

        let mut names = Dictionary::new();
        names.set("EmbeddedFiles", Object::Reference(node_id));
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_object_mut(root_id).unwrap().as_dict_mut().unwrap();
        catalog.set("Names", Object::Dictionary(names));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        // Read path.
        let err = read_attachments_packed(&bytes).unwrap_err();
        assert!(err.contains("maximum depth"), "unexpected error: {err}");

        // Write path (duplicate detection walks the existing tree too).
        let ops = r#"[{"name":"a.txt","offset":0,"length":2}]"#;
        let err = attach_files_json(&bytes, ops, b"hi", false).unwrap_err();
        assert!(err.contains("maximum depth"), "unexpected error: {err}");
    }
}
