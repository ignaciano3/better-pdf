//! Flatten engine: bake a field's appearance into its page and remove the
//! widget + AcroForm entry. Operates on existing `/AP` streams (see the M5
//! plan): fill (M4) generates appearances, flatten stamps them down.

use crate::fill::find_field;
use crate::forms;
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};

/// One widget to flatten: where it is and what appearance to stamp.
pub(crate) struct WidgetStamp {
    widget_id: ObjectId,
    page_id: ObjectId,
    rect: [f32; 4],
    /// Appearance stream id + its BBox, or None when the widget has no drawable AP.
    ap: Option<(ObjectId, [f32; 4])>,
}

/// (widget_id, page_id, rect) for one of a field's widgets.
pub(crate) struct RawWidget {
    pub(crate) id: ObjectId,
    pub(crate) page_id: ObjectId,
    pub(crate) rect: [f32; 4],
}

pub fn flatten_fields_json(
    data: &[u8],
    names_json: &str,
    compress: bool,
) -> Result<Vec<u8>, String> {
    let names: Vec<String> = serde_json::from_str(names_json).map_err(|e| e.to_string())?;
    let doc = crate::doc_io::load_pdf(data)?;
    let (field_ids, stamps) = flatten_resolve(&doc, &names)?;

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    flatten_apply(&mut inc, &field_ids, &stamps)?;

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Phase A: resolve fields + widget stamps against the immutable `doc`. Rejects XFA.
pub(crate) fn flatten_resolve(
    doc: &Document,
    names: &[String],
) -> Result<(Vec<ObjectId>, Vec<WidgetStamp>), String> {
    if forms::has_xfa(doc) {
        return Err(
            "XFA form detected: flattening is not supported because viewers render the XFA data, not the AcroForm values"
                .to_string(),
        );
    }
    let mut field_ids: Vec<ObjectId> = Vec::new();
    let mut stamps: Vec<WidgetStamp> = Vec::new();
    for name in names {
        let (field_id, dict) =
            find_field(doc, name).ok_or_else(|| format!("no such field: {name}"))?;
        field_ids.push(field_id);
        for w in field_widgets(doc, field_id, dict) {
            stamps.push(resolve_stamp(doc, w));
        }
    }
    Ok((field_ids, stamps))
}

/// Phase B: stamp widget appearances into page content and remove the fields.
pub(crate) fn flatten_apply(
    inc: &mut IncrementalDocument,
    field_ids: &[ObjectId],
    stamps: &[WidgetStamp],
) -> Result<(), String> {
    let mut counter = 0usize;
    for s in stamps {
        // Re-resolve the appearance at apply time: a fill earlier in the same
        // batched save may have just generated the /AP this stamp must use,
        // and that appearance lives in the incremental overlay, not in the
        // immutable document the plan was resolved against.
        let stamp = WidgetStamp {
            widget_id: s.widget_id,
            page_id: s.page_id,
            rect: s.rect,
            ap: resolve_ap_in_inc(inc, s).or(s.ap),
        };
        stamp_widget(inc, &stamp, &mut counter)?;
        remove_annot(inc, s.page_id, s.widget_id)?;
    }
    remove_fields(inc, field_ids)?;
    Ok(())
}

/// Look up an object in the incremental save: the overlay (objects added or
/// modified earlier in this save) wins over the loaded document.
fn inc_object(inc: &IncrementalDocument, id: ObjectId) -> Option<&Object> {
    inc.new_document
        .get_object(id)
        .ok()
        .or_else(|| inc.get_prev_documents().get_object(id).ok())
}

/// Apply-time appearance resolution (stream id + BBox) for a widget, seeing
/// the state fills earlier in the same save produced.
fn resolve_ap_in_inc(inc: &IncrementalDocument, s: &WidgetStamp) -> Option<(ObjectId, [f32; 4])> {
    let widget = inc_object(inc, s.widget_id)?.as_dict().ok()?;
    let ap = match widget.get(b"AP").ok()? {
        Object::Dictionary(d) => d,
        Object::Reference(id) => inc_object(inc, *id)?.as_dict().ok()?,
        _ => return None,
    };
    let n_id = match ap.get(b"N").ok()? {
        Object::Reference(id) => *id, // text/choice: N is the stream
        Object::Dictionary(states) => {
            // button: pick the /AS state's stream
            let as_name = widget.get(b"AS").ok()?.as_name().ok()?;
            states.get(as_name).ok()?.as_reference().ok()?
        }
        _ => return None,
    };
    let bbox = inc_object(inc, n_id)
        .and_then(|o| o.as_stream().ok())
        .and_then(|st| read_rect(&st.dict))
        .unwrap_or([0.0, 0.0, s.rect[2] - s.rect[0], s.rect[3] - s.rect[1]]);
    Some((n_id, bbox))
}

/// A field's widgets (id + page + rect). A field with no /Kids is its own widget.
pub(crate) fn field_widgets(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
) -> Vec<RawWidget> {
    let ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let ids = if ids.is_empty() { vec![field_id] } else { ids };
    ids.into_iter()
        .filter_map(|id| {
            let d = doc.get_dictionary(id).ok()?;
            let rect = read_rect(d)?;
            let page_id = d
                .get(b"P")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .or_else(|| find_page_of_annot(doc, id))?;
            Some(RawWidget { id, page_id, rect })
        })
        .collect()
}

pub(crate) fn read_rect(d: &Dictionary) -> Option<[f32; 4]> {
    let a = d.get(b"Rect").ok()?.as_array().ok()?;
    let mut r = [0f32; 4];
    for (i, v) in a.iter().enumerate().take(4) {
        r[i] = v.as_float().unwrap_or(0.0);
    }
    Some(r)
}

/// Find the page whose /Annots contains `annot` (fallback when /P is absent).
fn find_page_of_annot(doc: &Document, annot: ObjectId) -> Option<ObjectId> {
    for (_, &pid) in doc.get_pages().iter() {
        if let Ok(page) = doc.get_dictionary(pid)
            && let Ok(obj) = page.get(b"Annots")
            // /Annots may be an indirect reference to the array (Quartz does this).
            && let Ok((_, resolved)) = doc.dereference(obj)
            && let Ok(annots) = resolved.as_array()
            && annots.iter().any(|o| o.as_reference().ok() == Some(annot))
        {
            return Some(pid);
        }
    }
    None
}

/// Resolve the appearance stream (id + BBox) a widget currently shows.
fn resolve_stamp(doc: &Document, w: RawWidget) -> WidgetStamp {
    let ap = doc
        .get_dictionary(w.id)
        .ok()
        .and_then(|d| appearance_stream_id(doc, d))
        .map(|id| {
            let bbox = doc
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| read_rect(&s.dict))
                .unwrap_or([0.0, 0.0, w.rect[2] - w.rect[0], w.rect[3] - w.rect[1]]);
            (id, bbox)
        });
    WidgetStamp {
        widget_id: w.id,
        page_id: w.page_id,
        rect: w.rect,
        ap,
    }
}

/// The id of the appearance stream a widget currently shows.
fn appearance_stream_id(doc: &Document, widget: &Dictionary) -> Option<ObjectId> {
    let ap = forms::as_dict(doc, widget.get(b"AP").ok()?).ok()?;
    match ap.get(b"N").ok()? {
        Object::Reference(id) => Some(*id), // text/choice: N is the stream
        Object::Dictionary(states) => {
            // button: pick the /AS state's stream
            let as_name = widget.get(b"AS").ok()?.as_name().ok()?;
            states.get(as_name).ok()?.as_reference().ok()
        }
        _ => None,
    }
}

/// Stamp one widget's appearance onto its page.
fn stamp_widget(
    inc: &mut IncrementalDocument,
    s: &WidgetStamp,
    counter: &mut usize,
) -> Result<(), String> {
    let Some((ap_id, bbox)) = s.ap else {
        return Ok(()); // nothing to draw
    };
    let name = format!("bpdfAp{counter}");
    *counter += 1;

    // 1) register the appearance stream as an XObject in the page resources
    //    (handles both a /Resources reference and inline /Resources).
    register_xobject(inc, s.page_id, &name, ap_id)?;

    // 2) append a draw stream to the page contents (BBox -> Rect transform).
    let (bw, bh) = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
    let sx = if bw != 0.0 {
        (s.rect[2] - s.rect[0]) / bw
    } else {
        1.0
    };
    let sy = if bh != 0.0 {
        (s.rect[3] - s.rect[1]) / bh
    } else {
        1.0
    };
    let tx = s.rect[0] - bbox[0] * sx;
    let ty = s.rect[1] - bbox[1] * sy;
    let draw = format!("q {sx:.4} 0 0 {sy:.4} {tx:.2} {ty:.2} cm /{name} Do Q");
    let draw_id = inc.new_document.add_object(Object::Stream(
        Stream::new(Dictionary::new(), draw.into_bytes()),
    ));

    inc.opt_clone_object_to_new_document(s.page_id)
        .map_err(|e| e.to_string())?;
    let page = dict_mut(inc, s.page_id)?;
    let contents = page.get(b"Contents").map_err(|e| e.to_string())?.clone();
    let arr = match contents {
        Object::Array(mut a) => {
            a.push(Object::Reference(draw_id));
            a
        }
        single => vec![single, Object::Reference(draw_id)],
    };
    page.set("Contents", Object::Array(arr));
    Ok(())
}

/// Remove a widget reference from a page's /Annots.
fn remove_annot(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    widget: ObjectId,
) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(page_id)
        .map_err(|e| e.to_string())?;
    // /Annots may be an indirect reference to the array (Quartz does this);
    // resolve it against the previous revision, then write the filtered array
    // inline on the page.
    let annots: Option<Vec<Object>> = match dict_mut(inc, page_id)?.get(b"Annots") {
        Ok(Object::Array(a)) => Some(a.clone()),
        Ok(Object::Reference(id)) => {
            let id = *id;
            inc.get_prev_documents()
                .get_object(id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
        }
        _ => None,
    };
    if let Some(annots) = annots {
        let kept: Vec<Object> = annots
            .iter()
            .filter(|o| o.as_reference().ok() != Some(widget))
            .cloned()
            .collect();
        dict_mut(inc, page_id)?.set("Annots", Object::Array(kept));
    }
    Ok(())
}

/// Remove fields from the AcroForm /Fields (AcroForm inline in Catalog, or a ref).
///
/// `field_ids` are the *terminal* field object ids to drop. Because flattened
/// fields may be nested inside a hierarchical parent (`customer` → `name`), the
/// removal recurses through `/Kids`: a removed id is pruned wherever it lives,
/// and a non-terminal parent left with no field-kids is itself dropped.
fn remove_fields(inc: &mut IncrementalDocument, field_ids: &[ObjectId]) -> Result<(), String> {
    let remove: std::collections::HashSet<ObjectId> = field_ids.iter().copied().collect();
    let prev = inc.get_prev_documents();
    let root = prev
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    // Snapshot the top-level entries before mutating (the closure borrows `inc`).
    let acro_ref = cat.get(b"AcroForm").ok().and_then(|o| o.as_reference().ok());
    let entries: Vec<Object> = match cat.get(b"AcroForm") {
        Ok(o) => forms::as_dict(prev, o)
            .ok()
            .and_then(|a| a.get(b"Fields").and_then(|f| f.as_array()).ok())
            .cloned()
            .unwrap_or_default(),
        Err(_) => return Ok(()),
    };

    let mut kept: Vec<Object> = Vec::new();
    for e in entries {
        match e.as_reference() {
            Ok(id) => {
                if prune_field(inc, id, &remove, 0)? {
                    kept.push(e);
                }
            }
            _ => kept.push(e), // inline field dict (non-standard) — leave as-is
        }
    }

    match acro_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            dict_mut(inc, id)?.set("Fields", Object::Array(kept));
        }
        None => {
            // Inline AcroForm dict on the catalog.
            inc.opt_clone_object_to_new_document(root)
                .map_err(|e| e.to_string())?;
            let cat = dict_mut(inc, root)?;
            let acro = cat
                .get_mut(b"AcroForm")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            acro.set("Fields", Object::Array(kept));
        }
    }
    Ok(())
}

/// True when a `/Kids` entry resolves (through the incremental overlay) to a
/// child *field* — one carrying its own partial name `/T`.
fn kid_is_field_inc(inc: &IncrementalDocument, kid: &Object) -> bool {
    kid.as_reference()
        .ok()
        .and_then(|id| inc_object(inc, id))
        .and_then(|o| o.as_dict().ok())
        .map(|d| d.has(b"T"))
        .unwrap_or(false)
}

/// Recursively decide whether to keep the field at `id`, pruning removed
/// descendants from `/Kids`. Returns `true` to keep the node (its `/Kids`
/// rewritten in place when any field-kid was dropped), `false` to drop it —
/// either because it is itself in `remove`, or because it is a non-terminal
/// parent whose every field-kid was pruned away.
fn prune_field(
    inc: &mut IncrementalDocument,
    id: ObjectId,
    remove: &std::collections::HashSet<ObjectId>,
    depth: usize,
) -> Result<bool, String> {
    if remove.contains(&id) {
        return Ok(false);
    }
    if depth >= forms::MAX_PARENT_DEPTH {
        return Ok(true); // bound cyclic /Kids graphs
    }
    let kids: Vec<Object> = match inc_object(inc, id)
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Kids").ok())
        .and_then(|o| o.as_array().ok())
    {
        Some(a) => a.clone(),
        None => return Ok(true), // terminal leaf, not removed
    };
    if !kids.iter().any(|k| kid_is_field_inc(inc, k)) {
        return Ok(true); // terminal field whose kids are widget annotations
    }
    // Non-terminal: prune field-kids, preserving any non-field entries as-is.
    let mut kept: Vec<Object> = Vec::new();
    for k in kids {
        if kid_is_field_inc(inc, &k) {
            let kid_id = k.as_reference().map_err(|e| e.to_string())?;
            if prune_field(inc, kid_id, remove, depth + 1)? {
                kept.push(k);
            }
        } else {
            kept.push(k);
        }
    }
    if !kept.iter().any(|k| kid_is_field_inc(inc, k)) {
        return Ok(false); // every field-kid pruned → drop the emptied parent
    }
    inc.opt_clone_object_to_new_document(id)
        .map_err(|e| e.to_string())?;
    dict_mut(inc, id)?.set("Kids", Object::Array(kept));
    Ok(true)
}

/// Register `name -> ap_id` under the page's /Resources/XObject, whether
/// /Resources is a reference to a shared object or an inline dictionary on the
/// page. If the page has no /Resources, an inline one is created.
fn register_xobject(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    name: &str,
    ap_id: ObjectId,
) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(page_id)
        .map_err(|e| e.to_string())?;
    let res_ref = match dict_mut(inc, page_id)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match res_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            set_xobject(dict_mut(inc, id)?, name, ap_id);
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            if !page.has(b"Resources") {
                page.set("Resources", Object::Dictionary(Dictionary::new()));
            }
            let res = page
                .get_mut(b"Resources")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            set_xobject(res, name, ap_id);
        }
    }
    Ok(())
}

fn set_xobject(res: &mut Dictionary, name: &str, ap_id: ObjectId) {
    if !res.has(b"XObject") {
        res.set("XObject", Object::Dictionary(Dictionary::new()));
    }
    if let Ok(xobj) = res.get_mut(b"XObject").and_then(Object::as_dict_mut) {
        xobj.set(name.as_bytes().to_vec(), Object::Reference(ap_id));
    }
}

fn dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.new_document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::flatten_fields_json;
    use crate::fill::fill_fields_json;
    use lopdf::{Document, Object};

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn field_names(bytes: &[u8]) -> Vec<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn flatten_removes_field_and_stamps_page() {
        let filled = fill_fields_json(
            FICHA,
            r#"[{"name":"beneficiario.apellidos_nombres","value":"FLAT"}]"#,
            &[], false
        )
        .unwrap();
        let out = flatten_fields_json(&filled, r#"["beneficiario.apellidos_nombres"]"#, false).unwrap();

        // Append-only over the filled bytes.
        assert!(out.len() > filled.len());
        assert_eq!(&out[..filled.len()], &filled[..]);

        // Field is gone from the AcroForm.
        let names = field_names(&out);
        assert!(
            !names.iter().any(|n| n == "beneficiario.apellidos_nombres"),
            "field still present: {names:?}"
        );

        // Page /Contents is now an array; still a valid PDF.
        let doc = Document::load_mem(&out).unwrap();
        let (_, &pid) = doc.get_pages().iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        assert!(matches!(page.get(b"Contents"), Ok(Object::Array(_))));
    }

    #[test]
    fn flatten_removes_hierarchical_fields() {
        // A form whose fields are nested under parents (customer -> name): the
        // qualified children are flattened and their emptied parents pruned, so
        // no field survives in /AcroForm/Fields (pypdf fields_with_dots.pdf).
        const DOTS: &[u8] =
            include_bytes!("../../../tests/fixtures/pypdf/issues/fields_with_dots.pdf");
        let names = field_names(DOTS);
        assert!(
            names.iter().any(|n| n == "customer.name"),
            "expected a qualified child name: {names:?}"
        );
        let names_json = serde_json::to_string(&names).unwrap();
        let out = flatten_fields_json(DOTS, &names_json, false).unwrap();
        assert!(
            field_names(&out).is_empty(),
            "fields remain after flatten: {:?}",
            field_names(&out)
        );
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn flatten_unknown_field_errors() {
        let err = flatten_fields_json(FICHA, r#"["nope.nope"]"#, false).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }

    #[test]
    fn rejects_xfa_forms_on_flatten() {
        const FICHA_XFA: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-xfa.pdf");
        let err =
            flatten_fields_json(FICHA_XFA, r#"["beneficiario.apellidos_nombres"]"#, false).unwrap_err();
        assert!(err.contains("XFA"), "got: {err}");
    }
}
