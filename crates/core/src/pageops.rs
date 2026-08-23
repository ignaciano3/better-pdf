//! Assemble a new PDF from an ordered selection of pages across source PDFs.
//!
//! A single primitive — [`manipulate_pages_json`] — builds a brand-new PDF from
//! an ordered list of `{doc, page}` selections drawn from one or more source
//! documents. Merge, extract, reorder, remove and split all reduce to this.
use lopdf::{Document, Object, ObjectId, dictionary};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
struct DocDesc {
    offset: usize,
    length: usize,
}

#[derive(Deserialize)]
struct Sel {
    doc: usize,
    page: usize,
}

/// Attributes a /Page may inherit from an ancestor /Pages node.
const INHERITABLE: &[&[u8]] = &[b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Walk the page's /Parent chain; for each inheritable key the page lacks,
/// collect the nearest ancestor's value (references are resolved one level so
/// the carried value is self-contained-ish). A cycle guard bounds the walk.
fn resolve_inherited(doc: &Document, page_id: ObjectId) -> Vec<(Vec<u8>, Object)> {
    let mut found: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut current = Some(page_id);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 64 {
            break; // cycle guard
        }
        let dict = match doc.get_dictionary(id) {
            Ok(d) => d,
            Err(_) => break,
        };
        for &key in INHERITABLE {
            if found.iter().any(|(k, _)| k == key) {
                continue;
            }
            if let Ok(v) = dict.get(key) {
                let resolved = match v {
                    Object::Reference(r) => {
                        doc.get_object(*r).cloned().unwrap_or_else(|_| v.clone())
                    }
                    other => other.clone(),
                };
                found.push((key.to_vec(), resolved));
            }
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    found
}

/// AcroForm data captured from one source doc, in merged-id space.
#[derive(Clone)]
struct SourceForm {
    dr: Option<Object>,
    da: Option<Object>,
    top_fields: Vec<ObjectId>,
}

/// Capture a source doc's AcroForm /DR, /DA, and top-level field ids.
/// Call AFTER `renumber_objects_with` so the returned ids/refs are in
/// merged-id space, and BEFORE the objects are moved out of `doc`.
fn capture_source_form(doc: &Document) -> SourceForm {
    let mut out = SourceForm {
        dr: None,
        da: None,
        top_fields: Vec::new(),
    };
    let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return out;
    };
    let Ok(cat) = doc.get_dictionary(root) else {
        return out;
    };
    let af = match cat.get(b"AcroForm") {
        Ok(Object::Reference(r)) => match doc.get_dictionary(*r) {
            Ok(d) => d,
            Err(_) => return out,
        },
        Ok(Object::Dictionary(d)) => d,
        _ => return out,
    };
    out.dr = af.get(b"DR").ok().cloned();
    out.da = af.get(b"DA").ok().cloned();
    if let Ok(fields) = af.get(b"Fields").and_then(|o| o.as_array()) {
        for f in fields {
            if let Ok(id) = f.as_reference() {
                out.top_fields.push(id);
            }
        }
    }
    out
}

/// Walk a widget annotation's /Parent chain to the top-level field id.
/// Returns `annot` if it has no /Parent (terminal field == widget).
fn top_field_of(doc: &Document, annot: ObjectId) -> ObjectId {
    let mut cur = annot;
    for _ in 0..128 {
        let Ok(d) = doc.get_dictionary(cur) else {
            break;
        };
        match d.get(b"Parent").and_then(Object::as_reference) {
            Ok(p) => cur = p,
            Err(_) => break,
        }
    }
    cur
}

/// Reconstruct a working /AcroForm on the merged catalog from the field objects
/// whose widgets sit on kept pages. No-op when no kept widget maps to a field.
fn rebuild_acroform(
    merged: &mut Document,
    catalog_id: ObjectId,
    kept_pages: &[ObjectId],
    sources: &[SourceForm],
) {
    // Map each captured top-level field id to the source doc it came from.
    let mut field_src: HashMap<ObjectId, usize> = HashMap::new();
    for (si, s) in sources.iter().enumerate() {
        for &fid in &s.top_fields {
            field_src.entry(fid).or_insert(si);
        }
    }
    if field_src.is_empty() {
        return;
    }

    // Find top-level fields reachable from widgets on kept pages, in page order.
    let mut kept_fields: Vec<ObjectId> = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for &pid in kept_pages {
        let annot_ids: Vec<ObjectId> = match merged
            .get_dictionary(pid)
            .ok()
            .and_then(|pd| pd.get(b"Annots").ok())
            .and_then(|o| o.as_array().ok())
        {
            Some(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
            None => continue,
        };
        for aid in annot_ids {
            let top = top_field_of(merged, aid);
            if field_src.contains_key(&top) && seen.insert(top) {
                kept_fields.push(top);
            }
        }
    }
    if kept_fields.is_empty() {
        return;
    }

    // Detect partial-name collisions across SOURCE docs and rename them with a
    // per-source prefix so each field stays independently addressable.
    fn partial_name(doc: &Document, id: ObjectId) -> Option<String> {
        let d = doc.get_dictionary(id).ok()?;
        let t = d.get(b"T").ok()?.as_str().ok()?;
        Some(String::from_utf8_lossy(t).into_owned())
    }
    // name -> set of source indices that use it (among kept fields)
    let mut name_sources: HashMap<String, HashSet<usize>> = HashMap::new();
    for &fid in &kept_fields {
        if let (Some(name), Some(&si)) = (partial_name(merged, fid), field_src.get(&fid)) {
            name_sources.entry(name).or_default().insert(si);
        }
    }
    for &fid in &kept_fields {
        let (Some(name), Some(&si)) = (partial_name(merged, fid), field_src.get(&fid)) else {
            continue;
        };
        let collides = name_sources
            .get(&name)
            .map(|s| s.len() > 1)
            .unwrap_or(false);
        if collides {
            let new_name = format!("d{si}_{name}");
            if let Ok(d) = merged.get_dictionary_mut(fid) {
                d.set("T", Object::string_literal(new_name));
            }
        }
    }

    let fields: Vec<Object> = kept_fields
        .iter()
        .map(|&id| Object::Reference(id))
        .collect();

    // Merge /DR /Font entries across sources (first-writer-wins per name).
    let mut merged_fonts = lopdf::Dictionary::new();
    let mut da: Option<Object> = None;
    for s in sources {
        if da.is_none()
            && let Some(d) = &s.da
        {
            da = Some(d.clone());
        }
        let Some(dr_obj) = &s.dr else { continue };
        let dr_dict = match dr_obj {
            Object::Reference(r) => merged.get_dictionary(*r).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        };
        let Some(dr_dict) = dr_dict else { continue };
        let font_obj = dr_dict.get(b"Font").ok().cloned();
        let font_dict = match font_obj {
            Some(Object::Reference(r)) => merged.get_dictionary(r).ok().cloned(),
            Some(Object::Dictionary(d)) => Some(d),
            _ => None,
        };
        if let Some(fd) = font_dict {
            for (k, v) in fd.iter() {
                if !merged_fonts.has(k) {
                    merged_fonts.set(k.to_vec(), v.clone());
                }
            }
        }
    }

    let mut dr = lopdf::Dictionary::new();
    if !merged_fonts.as_hashmap().is_empty() {
        dr.set("Font", Object::Dictionary(merged_fonts));
    }

    let mut acroform = dictionary! {
        "Fields" => Object::Array(fields),
        "NeedAppearances" => Object::Boolean(true),
    };
    if !dr.as_hashmap().is_empty() {
        acroform.set("DR", Object::Dictionary(dr));
    }
    if let Some(da) = da {
        acroform.set("DA", da);
    }
    let acroform_id = merged.add_object(acroform);
    if let Ok(cat) = merged.get_dictionary_mut(catalog_id) {
        cat.set("AcroForm", Object::Reference(acroform_id));
    }
}

/// One parsed source document, ready for plan assembly: objects already moved
/// out of the loader's `Document` (ids renumbered into merged space), page ids
/// recorded, AcroForm data captured.
struct PreparedSource {
    objects: std::collections::BTreeMap<ObjectId, Object>,
    max_id: u32,
    pages: Vec<ObjectId>,
    form: SourceForm,
}

/// Parse one source PDF and shift its ids into a disjoint range starting at
/// `next`; returns the prepared source and the next free id. Shared by
/// [`manipulate_pages_json`] and [`split_pages_packed`].
fn prepare_source(bytes: &[u8], next: u32) -> Result<(PreparedSource, u32), String> {
    let mut doc = crate::doc_io::load_pdf(bytes)?;

    // Resolve inherited attrs onto each page BEFORE renumber/move, while the
    // /Parent chain is still intact. Only set keys the page itself lacks.
    let pre_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for &pid in &pre_ids {
        let inh = resolve_inherited(&doc, pid);
        if let Ok(pd) = doc.get_dictionary_mut(pid) {
            for (k, v) in inh {
                if !pd.has(&k) {
                    pd.set(k, v);
                }
            }
        }
    }

    // Shift this doc's object ids (and every internal reference) into a
    // disjoint range starting at `next`.
    doc.renumber_objects_with(next);
    let next_after = doc.max_id + 1;

    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();

    // Capture AcroForm data while ids are renumbered but objects still live.
    let form = capture_source_form(&doc);

    Ok((
        PreparedSource {
            objects: std::mem::take(&mut doc.objects),
            max_id: doc.max_id,
            pages,
            form,
        },
        next_after,
    ))
}

/// Build a brand-new PDF from `plan` (ordered `{doc, page}` selections) against
/// the prepared sources. Consumes the prepared objects (they are moved into the
/// output document, so callers wanting multiple outputs must clone per call —
/// see [`split_pages_packed`]).
fn assemble_from_prepared(
    sources: &mut [PreparedSource],
    plan: &[Sel],
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    if plan.is_empty() {
        return Err("no pages selected".to_string());
    }

    let mut merged = Document::with_version("1.7");
    let mut next: u32 = 1;
    let mut per_doc_pages: Vec<Vec<ObjectId>> = Vec::with_capacity(sources.len());
    let mut moved_forms: Vec<SourceForm> = Vec::with_capacity(sources.len());

    for s in sources.iter_mut() {
        // Bulk-move every object into the merged doc.
        merged.objects.extend(std::mem::take(&mut s.objects));
        next = s.max_id + 1;
        per_doc_pages.push(std::mem::take(&mut s.pages));
        moved_forms.push(std::mem::replace(
            &mut s.form,
            SourceForm { dr: None, da: None, top_fields: Vec::new() },
        ));
    }

    // CRITICAL: set max_id from the loop's final `next` BEFORE any
    // `new_object_id`/`add_object` call, so fresh ids never collide with the
    // moved objects.
    merged.max_id = next.saturating_sub(1);

    let pages_id = merged.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(plan.len());
    let mut used: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    let mut kept_pages: Vec<ObjectId> = Vec::with_capacity(plan.len());
    for s in plan {
        let pages = per_doc_pages
            .get(s.doc)
            .ok_or_else(|| format!("doc index {} out of range", s.doc))?;
        let src_pid = *pages
            .get(s.page)
            .ok_or_else(|| format!("page index {} out of range", s.page))?;
        // A page selected more than once must become a distinct object so the
        // output tree has independent /Parent links. Shallow-clone the page
        // dict (shared Contents/Resources references are fine).
        let pid = if used.contains(&src_pid) {
            let cloned = merged
                .get_dictionary(src_pid)
                .map_err(|e| e.to_string())?
                .clone();
            merged.add_object(Object::Dictionary(cloned))
        } else {
            used.insert(src_pid);
            src_pid
        };
        if let Ok(pd) = merged.get_dictionary_mut(pid) {
            pd.set("Parent", Object::Reference(pages_id));
        }
        kids.push(Object::Reference(pid));
        kept_pages.push(pid);
    }

    let count = kids.len() as i64;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(count),
        }),
    );
    let catalog_id = merged.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    merged.trailer.set("Root", Object::Reference(catalog_id));

    rebuild_acroform(&mut merged, catalog_id, &kept_pages, &moved_forms);

    // Drop the old per-source catalogs/pages-trees and any unselected pages.
    // Everything reachable from the new Root (Pages tree + selected pages +
    // their content/resources/annots) is retained.
    merged.prune_objects();

    crate::compress::serialize_document(&mut merged, compress, object_streams)
}

/// Assemble a new PDF from an ordered page selection across the source PDFs
/// packed into `docs_blob`.
///
/// * `docs_blob` — the concatenated bytes of every source PDF.
/// * `docs_json` — JSON array of `{"offset","length"}` slicing `docs_blob` into docs.
/// * `plan_json` — JSON array of `{"doc","page"}` (both 0-based) giving the
///   ordered output pages. Duplicates are allowed and yield distinct pages.
pub fn manipulate_pages_json(
    docs_blob: &[u8],
    docs_json: &str,
    plan_json: &str,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    let descs: Vec<DocDesc> =
        serde_json::from_str(docs_json).map_err(|e| format!("invalid docs: {e}"))?;
    let plan: Vec<Sel> =
        serde_json::from_str(plan_json).map_err(|e| format!("invalid plan: {e}"))?;

    let mut sources: Vec<PreparedSource> = Vec::with_capacity(descs.len());
    let mut next: u32 = 1;
    for d in &descs {
        let end = d
            .offset
            .checked_add(d.length)
            .ok_or("doc range out of bounds")?;
        if end > docs_blob.len() {
            return Err("doc range out of bounds".to_string());
        }
        let (prepared, next_after) = prepare_source(&docs_blob[d.offset..end], next)?;
        sources.push(prepared);
        next = next_after;
    }

    assemble_from_prepared(&mut sources, &plan, compress, object_streams)
}

/// Split a single PDF into one single-page PDF per page, in document order.
///
/// The source is parsed exactly once; each output reuses that prepared object
/// set (cloned per output), so splitting N pages costs one full parse instead
/// of N.
///
/// Wire format (same framing as `read_attachments`):
/// `[u32 LE json_len][json [{"offset","length"}; n]][concatenated outputs]`,
/// where each entry's offset indexes into the trailing byte section.
pub fn split_pages_packed(
    data: &[u8],
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    let (prepared, _) = prepare_source(data, 1)?;
    let page_count = prepared.pages.len();

    let mut outs: Vec<Vec<u8>> = Vec::with_capacity(page_count);
    for page in 0..page_count {
        // Clone so every output assembles from identical state (assembly
        // consumes the prepared objects it is given).
        let single = PreparedSource {
            objects: prepared.objects.clone(),
            max_id: prepared.max_id,
            pages: prepared.pages.clone(),
            form: prepared.form.clone(),
        };
        outs.push(assemble_from_prepared(
            &mut [single],
            &[Sel { doc: 0, page }],
            compress,
            object_streams,
        )?);
    }

    pack_documents(outs)
}

/// Pack whole-PDF outputs into `[u32 LE json_len][json table][bytes]`.
fn pack_documents(outs: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut offset = 0usize;
    let mut entries = String::from("[");
    for (i, o) in outs.iter().enumerate() {
        if i > 0 {
            entries.push(',');
        }
        entries.push_str(&format!(r#"{{"offset":{},"length":{}}}"#, offset, o.len()));
        offset += o.len();
    }
    entries.push(']');

    let mut packed = Vec::with_capacity(4 + entries.len() + offset);
    packed.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    packed.extend_from_slice(entries.as_bytes());
    for o in outs {
        packed.extend_from_slice(&o);
    }
    Ok(packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn page_count(bytes: &[u8]) -> usize {
        Document::load_mem(bytes).unwrap().get_pages().len()
    }

    // Concatenate sources, build the docs_json table.
    fn pack(docs: &[&[u8]]) -> (Vec<u8>, String) {
        let mut blob = Vec::new();
        let mut table = String::from("[");
        for (i, d) in docs.iter().enumerate() {
            if i > 0 {
                table.push(',');
            }
            table.push_str(&format!(
                r#"{{"offset":{},"length":{}}}"#,
                blob.len(),
                d.len()
            ));
            blob.extend_from_slice(d);
        }
        table.push(']');
        (blob, table)
    }

    #[test]
    fn merge_two_copies_doubles_page_count() {
        let n = page_count(FICHA);
        let (blob, docs) = pack(&[FICHA, FICHA]);
        // plan = all pages of doc 0 then all pages of doc 1
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan, false, false).unwrap();
        assert_eq!(page_count(&out), 2 * n);
    }

    #[test]
    fn extract_single_page() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":0}]"#, false, false).unwrap();
        assert_eq!(page_count(&out), 1);
        // MediaBox present on the extracted page (inherited attrs resolved)
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert!(
            doc.get_dictionary(pid).unwrap().has(b"MediaBox"),
            "extracted page must carry MediaBox"
        );
    }

    #[test]
    fn reorder_preserves_count() {
        let n = page_count(FICHA);
        if n >= 2 {
            let (blob, docs) = pack(&[FICHA]);
            let out =
                manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":1},{"doc":0,"page":0}]"#, false, false)
                    .unwrap();
            assert_eq!(page_count(&out), 2);
        }
    }

    #[test]
    fn errors_on_empty_plan() {
        let (blob, docs) = pack(&[FICHA]);
        assert!(manipulate_pages_json(&blob, &docs, "[]", false, false).is_err());
    }

    #[test]
    fn errors_on_page_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":9999}]"#, false, false);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_doc_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":5,"page":0}]"#, false, false);
        assert!(r.unwrap_err().contains("doc"));
    }

    #[test]
    fn merge_rebuilds_interactive_acroform() {
        // FICHA is an AcroForm PDF. Merging two copies must yield an output
        // whose catalog has an /AcroForm with a non-empty /Fields array.
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan, false, false).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = cat
            .get(b"AcroForm")
            .expect("merged output must have /AcroForm");
        let af = match af {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("AcroForm must be a dict or ref"),
        };
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        assert!(!fields.is_empty(), "/Fields must be non-empty");
        assert!(
            af.get(b"NeedAppearances")
                .ok()
                .and_then(|o| o.as_bool().ok())
                == Some(true),
            "NeedAppearances must be true"
        );
    }

    #[test]
    fn merge_acroform_has_dr_fonts_and_da() {
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan, false, false).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!(),
        };
        // /DR present with a /Font subdict that has at least one entry.
        let dr = af.get(b"DR").expect("AcroForm must carry /DR");
        let dr = match dr {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("DR must be dict/ref"),
        };
        let fonts = dr.get(b"Font").expect("DR must carry /Font");
        let fonts = match fonts {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("DR/Font must be dict/ref"),
        };
        assert!(!fonts.as_hashmap().is_empty(), "DR/Font must have entries");
        assert!(af.has(b"DA"), "AcroForm should carry a /DA");
    }

    #[test]
    fn merge_self_renames_colliding_field_names() {
        // Merging FICHA with itself: every field name appears in both sources,
        // so all kept top-level fields must be prefixed d0_/d1_ — yielding no
        // duplicate /T values among the rebuilt /Fields.
        let (blob, docs) = pack(&[FICHA, FICHA]);
        let n = page_count(FICHA);
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan, false, false).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let cat = doc.get_dictionary(root).unwrap();
        let af = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!(),
        };
        let fields = af.get(b"Fields").unwrap().as_array().unwrap();
        let mut names: Vec<String> = Vec::new();
        for f in fields {
            let fd = doc.get_dictionary(f.as_reference().unwrap()).unwrap();
            if let Ok(t) = fd.get(b"T").and_then(|o| o.as_str()) {
                names.push(String::from_utf8_lossy(t).into_owned());
            }
        }
        let unique: HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "no duplicate top-level /T names");
        assert!(
            names.iter().any(|n| n.starts_with("d0_"))
                && names.iter().any(|n| n.starts_with("d1_")),
            "colliding names must be per-source prefixed"
        );
    }

    #[test]
    fn top_field_of_walks_parent_to_root() {
        // Build a tiny doc: top field A (no Parent) -> kid widget W (Parent A).
        let mut d = Document::with_version("1.7");
        let a = d.new_object_id();
        let w = d.new_object_id();
        d.objects.insert(
            a,
            Object::Dictionary(dictionary! { "T" => Object::string_literal("A") }),
        );
        d.objects.insert(
            w,
            Object::Dictionary(dictionary! {
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "Parent" => Object::Reference(a),
            }),
        );
        assert_eq!(top_field_of(&d, w), a, "widget resolves to its top field");
        assert_eq!(top_field_of(&d, a), a, "a top field resolves to itself");
    }

    fn unpack_docs(packed: &[u8]) -> Vec<Vec<u8>> {
        let len = u32::from_le_bytes(packed[0..4].try_into().unwrap()) as usize;
        let json = std::str::from_utf8(&packed[4..4 + len]).unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
        let base = 4 + len;
        entries
            .iter()
            .map(|e| {
                let off = e["offset"].as_u64().unwrap() as usize;
                let l = e["length"].as_u64().unwrap() as usize;
                packed[base + off..base + off + l].to_vec()
            })
            .collect()
    }

    #[test]
    fn split_produces_one_single_page_pdf_per_page() {
        let n = page_count(FICHA);
        let outs = unpack_docs(&split_pages_packed(FICHA, true, false).unwrap());
        assert_eq!(outs.len(), n);
        for out in &outs {
            assert_eq!(page_count(out), 1);
        }
    }

    #[test]
    fn split_outputs_are_byte_identical_to_per_page_manipulate() {
        // The equivalence contract: batched split must emit exactly what the
        // generic assembler emits for each single-page selection.
        let outs = unpack_docs(&split_pages_packed(FICHA, true, false).unwrap());
        let (blob, docs) = pack(&[FICHA]);
        for (p, out) in outs.iter().enumerate() {
            let plan = format!(r#"[
                {{"doc":0,"page":{p}}}
            ]"#);
            let expected = manipulate_pages_json(&blob, &docs, &plan, true, false).unwrap();
            assert_eq!(out, &expected, "split output {p} must be byte-identical");
        }
    }

    #[test]
    fn split_honors_object_streams_flag() {
        let outs = unpack_docs(&split_pages_packed(FICHA, true, true).unwrap());
        assert!(
            outs[0].windows(6).any(|w| w == b"ObjStm"),
            "expected an /ObjStm in object-stream split output"
        );
        assert_eq!(Document::load_mem(&outs[0]).unwrap().get_pages().len(), 1);
    }

    #[test]
    fn duplicate_page_selection_produces_two_distinct_pages() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":0},{"doc":0,"page":0}]"#, false, false)
            .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ids: Vec<_> = doc.get_pages().into_values().collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(
            ids[0], ids[1],
            "duplicate selection must yield distinct page objects"
        );
    }

    #[test]
    fn manipulate_pages_object_streams_shrinks_merge() {
        // Merge two copies of the FICHA form (many field/annotation dicts to pack).
        // Build the concatenated blob + offset table exactly as the existing merge
        // tests do (two entries, both FICHA), and a plan selecting every page of
        // both docs. Reuse whatever helper/inline shape the neighbouring merge test
        // uses; the assertion below is size + /ObjStm + round-trip, not exact bytes.
        let blob = [FICHA, FICHA].concat();
        let docs_json = format!(
            r#"[{{"offset":0,"length":{}}},{{"offset":{},"length":{}}}]"#,
            FICHA.len(),
            FICHA.len(),
            FICHA.len()
        );
        // Select page 0 of each doc (both fixtures have at least one page).
        let plan_json = r#"[{"doc":0,"page":0},{"doc":1,"page":0}]"#;

        let packed =
            manipulate_pages_json(&blob, &docs_json, plan_json, true, true).unwrap();
        let plain =
            manipulate_pages_json(&blob, &docs_json, plan_json, true, false).unwrap();

        assert!(
            packed.len() < plain.len(),
            "object-stream merge {} should be smaller than {}",
            packed.len(),
            plain.len()
        );
        assert!(
            packed.windows(6).any(|w| w == b"ObjStm"),
            "expected an /ObjStm in object-stream merge output"
        );
        assert_eq!(Document::load_mem(&packed).unwrap().get_pages().len(), 2);
    }
}
