//! Inject builder-defined AcroForm fields into an already-loaded PDF via an
//! incremental update. When the target document has no `/AcroForm`, a fresh one
//! is created and attached to the catalog; when one already exists, the new field
//! refs and font aliases are merged into it (appending `/Fields`, merging
//! `/DR/Font`) while leaving `/DA` and `/NeedAppearances` intact.

use crate::create::{
    BuiltField, FieldDef, FieldFont, FontDesc, build_one_field, da_font_alias, validate_fields,
};
use crate::doc_io::load_pdf;
use crate::draw::{append_annot_to_page, font_dict};
use crate::fonts::{BuiltFont, EmbeddedFontInput, build_embedded_font};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, dictionary};
use std::collections::{BTreeSet, HashMap};

pub fn inject_fields_json(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
    compress: bool,
) -> Result<Vec<u8>, String> {
    let fields: Vec<FieldDef> =
        serde_json::from_str(fields_json).map_err(|e| format!("invalid fields JSON: {e}"))?;
    if fields.is_empty() {
        return Ok(data.to_vec());
    }
    let font_descs: Vec<FontDesc> =
        serde_json::from_str(fonts_json).map_err(|e| format!("invalid fonts JSON: {e}"))?;

    // Validate the font-blob byte ranges up front (NOT covered by
    // `create::validate_fields`), so the later `fonts[offset..offset+length]`
    // slices cannot panic.
    for fd in &font_descs {
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        if end > fonts.len() {
            return Err("font range out of bounds".to_string());
        }
    }
    // Reject unknown standard-14 field fonts (NOT covered by validate_fields),
    // so `resolve_std_aliases`' `da_font_alias(..).expect(..)` cannot panic.
    validate_field_std14_fonts(&fields)?;

    let doc = load_pdf(data)?;

    // Collision check against existing top-level field names BEFORE mutating.
    let existing_names = existing_field_names(&doc)?;
    for f in &fields {
        let name = field_name(f);
        if existing_names.contains(name) {
            return Err(format!(
                "field name '{name}' already exists in this document"
            ));
        }
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Resolve target page ids (0-based index into sorted pages), same as draw.rs.
    let page_ids: Vec<ObjectId> = {
        let prev = inc.get_prev_documents();
        let mut sorted: Vec<(u32, ObjectId)> = prev.get_pages().into_iter().collect();
        sorted.sort_by_key(|(n, _)| *n);
        sorted.into_iter().map(|(_, id)| id).collect()
    };
    // Full per-field validation against the ACTUAL page count — every radio
    // option's page included — mirroring create's trust boundary. Runs before
    // any object is added/cloned, so a rejected request produces zero mutation.
    validate_fields(&fields, page_ids.len(), &font_descs)?;

    // Existing /DR/Font alias keys (to uniquify against). Empty when no AcroForm.
    let existing_aliases = existing_dr_aliases(&inc);

    // Build embedded fonts into new_document; map font_id -> (type0_id, BuiltFont).
    let embedded_fonts = build_embedded_for_fields(&mut inc, &fields, &font_descs, fonts)?;

    // Resolve standard-14 + embedded aliases, add /DR font objects to new_document.
    let mut dr_additions: Vec<(String, ObjectId)> = Vec::new();
    let std_aliases = resolve_std_aliases(&mut inc, &fields, &existing_aliases, &mut dr_additions);
    let emb_aliases = resolve_embedded_aliases(
        &embedded_fonts,
        &existing_aliases,
        &std_aliases,
        &mut dr_additions,
    );

    // Build each field and wire widgets onto pages.
    let mut acro_field_ids: Vec<ObjectId> = Vec::new();
    for f in &fields {
        let font = field_font(
            f,
            &std_aliases,
            &emb_aliases,
            &embedded_fonts,
            &font_descs,
            fonts,
        );
        let built: BuiltField = build_one_field(&mut inc.new_document, f, &page_ids, font)?;
        acro_field_ids.push(built.top_field_id);
        for (page_idx, widget_id) in built.widgets {
            // Clone the target page into new_document before mutating its /Annots.
            inc.opt_clone_object_to_new_document(page_ids[page_idx])
                .map_err(|e| e.to_string())?;
            append_annot_to_page(&mut inc, page_ids[page_idx], widget_id)?;
        }
    }

    // Merge into an existing /AcroForm (append /Fields, merge /DR/Font) or
    // create a fresh one when the document has none.
    merge_or_create_acroform(&mut inc, &acro_field_ids, &dr_additions)?;

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Reject unknown standard-14 field fonts. `create::validate_fields` covers
/// embedded `font_id` range and comb rules, but NOT standard-14 name validity,
/// which `resolve_std_aliases` relies on (`da_font_alias(..).expect(..)`).
fn validate_field_std14_fonts(fields: &[FieldDef]) -> Result<(), String> {
    for f in fields {
        match f {
            // Embedded-font text fields don't use a standard-14 alias.
            FieldDef::Text {
                font_id: Some(_), ..
            } => {}
            FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
                let base = font.as_deref().unwrap_or("Helvetica");
                if da_font_alias(base).is_none()
                    || crate::appearance::standard_14_widths(base).is_none()
                {
                    return Err(format!("unknown field font: {base}"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The top-level `/T` name of a field.
fn field_name(f: &FieldDef) -> &str {
    match f {
        FieldDef::Text { name, .. }
        | FieldDef::CheckBox { name, .. }
        | FieldDef::RadioGroup { name, .. }
        | FieldDef::Choice { name, .. }
        | FieldDef::Signature { name, .. } => name,
    }
}

/// Top-level field `/T` names already in the document (empty if no AcroForm).
fn existing_field_names(doc: &Document) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;
    let cat = doc.get_dictionary(root).map_err(|e| e.to_string())?;
    let acro = match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    if let Some(acro) = acro
        && let Ok(arr) = acro.get(b"Fields").and_then(Object::as_array)
    {
        for f in arr {
            if let Ok(id) = f.as_reference()
                && let Ok(fd) = doc.get_dictionary(id)
                && let Ok(t) = fd.get(b"T").and_then(Object::as_str)
            {
                names.insert(String::from_utf8_lossy(t).into_owned());
            }
        }
    }
    Ok(names)
}

/// Test-only wrapper so the `tests` module (and sibling-module tests) can reuse
/// the top-level field-name walker.
#[cfg(test)]
pub(crate) fn test_field_names(doc: &Document) -> BTreeSet<String> {
    existing_field_names(doc).unwrap()
}

/// Existing `/DR/Font` alias keys, so injected aliases can avoid collisions.
/// Empty when the document has no AcroForm (the current no-AcroForm path).
fn existing_dr_aliases(inc: &IncrementalDocument) -> BTreeSet<String> {
    let doc = inc.get_prev_documents();
    let mut set = BTreeSet::new();
    let resolve = |o: &Object| -> Option<Dictionary> {
        match o {
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        }
    };
    let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return set;
    };
    let Ok(cat) = doc.get_dictionary(root) else {
        return set;
    };
    let Some(acro) = cat.get(b"AcroForm").ok().and_then(&resolve) else {
        return set;
    };
    let Some(dr) = acro.get(b"DR").ok().and_then(&resolve) else {
        return set;
    };
    let Some(font) = dr.get(b"Font").ok().and_then(&resolve) else {
        return set;
    };
    for (k, _) in font.iter() {
        set.insert(String::from_utf8_lossy(k).into_owned());
    }
    set
}

/// Build embedded fonts referenced by text fields into `inc.new_document`.
/// Mirrors the create pre-pass but adds objects via `inc.new_document.add_object`.
fn build_embedded_for_fields(
    inc: &mut IncrementalDocument,
    fields: &[FieldDef],
    font_descs: &[FontDesc],
    fonts: &[u8],
) -> Result<HashMap<usize, (ObjectId, BuiltFont)>, String> {
    let mut used_per_font: HashMap<usize, BTreeSet<char>> = HashMap::new();
    for f in fields {
        if let FieldDef::Text {
            font_id: Some(i),
            value,
            default_value,
            ..
        } = f
        {
            let set = used_per_font.entry(*i).or_default();
            if let Some(v) = value {
                set.extend(v.chars());
            }
            if let Some(dv) = default_value {
                set.extend(dv.chars());
            }
        }
    }
    let mut ids: Vec<usize> = used_per_font.keys().copied().collect();
    ids.sort_unstable();
    let mut out = HashMap::new();
    for id in ids {
        let fd = &font_descs[id];
        let bytes = &fonts[fd.offset..fd.offset + fd.length];
        let input = EmbeddedFontInput {
            data: bytes,
            subset: fd.subset,
            used_chars: used_per_font.remove(&id).unwrap_or_default(),
        };
        let mut add = |o: Object| inc.new_document.add_object(o);
        let built = build_embedded_font(&mut add, &input)?;
        out.insert(id, built);
    }
    Ok(out)
}

/// Assign a unique alias for each standard-14 base font used, add its font dict
/// to new_document, and record `(alias, id)` in `dr_additions`. Returns
/// base-font -> `(alias, id)`.
fn resolve_std_aliases(
    inc: &mut IncrementalDocument,
    fields: &[FieldDef],
    existing: &BTreeSet<String>,
    dr_additions: &mut Vec<(String, ObjectId)>,
) -> HashMap<String, (String, ObjectId)> {
    let mut needed: BTreeSet<&str> = BTreeSet::new();
    needed.insert("Helvetica");
    for f in fields {
        // Only standard-14 text/choice fields need a /DR alias here; embedded
        // (`font_id`) text fields are handled by `resolve_embedded_aliases`.
        match f {
            FieldDef::Text {
                font,
                font_id: None,
                ..
            } => {
                needed.insert(font.as_deref().unwrap_or("Helvetica"));
            }
            FieldDef::Choice { font, .. } => {
                needed.insert(font.as_deref().unwrap_or("Helvetica"));
            }
            _ => {}
        }
    }
    let mut used_aliases: BTreeSet<String> = existing.clone();
    let mut map = HashMap::new();
    for base in needed {
        let canonical = da_font_alias(base).expect("validated font");
        let alias = uniquify(canonical, &mut used_aliases);
        let id = inc
            .new_document
            .add_object(Object::Dictionary(font_dict(base)));
        dr_additions.push((alias.clone(), id));
        map.insert(base.to_string(), (alias, id));
    }
    map
}

/// Assign unique `/BPF<n>` aliases for embedded fonts and record them in `/DR`.
fn resolve_embedded_aliases(
    embedded_fonts: &HashMap<usize, (ObjectId, BuiltFont)>,
    existing: &BTreeSet<String>,
    std_aliases: &HashMap<String, (String, ObjectId)>,
    dr_additions: &mut Vec<(String, ObjectId)>,
) -> HashMap<usize, String> {
    let mut used: BTreeSet<String> = existing.clone();
    for (a, _) in std_aliases.values() {
        used.insert(a.clone());
    }
    let mut map = HashMap::new();
    let mut ids: Vec<usize> = embedded_fonts.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let alias = uniquify(&format!("BPF{id}"), &mut used);
        let (type0_id, _) = embedded_fonts[&id];
        dr_additions.push((alias.clone(), type0_id));
        map.insert(id, alias);
    }
    map
}

/// Return `base` if unused, else `base_1`, `base_2`, … Records the chosen alias.
fn uniquify(base: &str, used: &mut BTreeSet<String>) -> String {
    if !used.contains(base) {
        used.insert(base.to_string());
        return base.to_string();
    }
    let mut n = 1;
    loop {
        let cand = format!("{base}_{n}");
        if !used.contains(&cand) {
            used.insert(cand.clone());
            return cand;
        }
        n += 1;
    }
}

/// Build the `FieldFont` for a field from the resolved alias maps, mirroring
/// `create::resolve_create_font` variant selection exactly.
fn field_font<'a>(
    f: &FieldDef,
    std_aliases: &'a HashMap<String, (String, ObjectId)>,
    emb_aliases: &'a HashMap<usize, String>,
    embedded_fonts: &'a HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &[FontDesc],
    fonts: &'a [u8],
) -> Option<FieldFont<'a>> {
    match f {
        FieldDef::Text {
            font_id: Some(i), ..
        } => {
            let (type0_id, built) = &embedded_fonts[i];
            let fd = &font_descs[*i];
            Some(FieldFont::Embedded {
                alias: &emb_aliases[i],
                type0_id: *type0_id,
                built,
                bytes: &fonts[fd.offset..fd.offset + fd.length],
            })
        }
        FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
            let base = font.as_deref().unwrap_or("Helvetica");
            let (alias, font_ref) = &std_aliases[base];
            Some(FieldFont::Standard {
                alias,
                font_ref: *font_ref,
            })
        }
        _ => None,
    }
}

/// Where the new font aliases must be written, discovered from the existing
/// `/AcroForm`'s `/DR`. `Indirect`/`InlineInDr` reference standalone objects that
/// are cloned+edited on their own; `InlineInAcro` is written into the AcroForm
/// dictionary itself (creating `/DR` and/or `/Font` as needed).
#[derive(Clone, Copy)]
enum FontPlan {
    /// `/DR/Font` is its own indirect object.
    Indirect(ObjectId),
    /// `/DR` is an indirect object whose `/Font` is inline (or absent) within it.
    InlineInDr(ObjectId),
    /// `/DR` is inline in the AcroForm (or absent); write `/DR/Font` there.
    InlineInAcro,
}

/// Attach fields to an existing `/AcroForm` (append `/Fields`, merge `/DR/Font`)
/// or create one if absent. Uses the fill.rs clone-and-edit pattern for both the
/// indirect-reference and inline-in-catalog AcroForm storage forms, and clones
/// indirect `/Fields`, `/DR`, or `/DR/Font` objects before editing them (mirroring
/// `draw::append_annot_to_page`). Leaves `/DA` and `/NeedAppearances` untouched
/// on an existing form (only seeds a default `/DA` when absent).
fn merge_or_create_acroform(
    inc: &mut IncrementalDocument,
    field_ids: &[ObjectId],
    dr_additions: &[(String, ObjectId)],
) -> Result<(), String> {
    let root = inc
        .get_prev_documents()
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;

    // Discover the AcroForm storage form and the shape of /Fields and /DR, all
    // by reading the (unmodified) previous document. Only Copy data escapes the
    // borrow, so we can mutate `inc` afterwards.
    let acro_ref: Option<ObjectId>;
    let fields_indirect: Option<ObjectId>;
    let da_absent: bool;
    let font_plan: FontPlan;
    {
        let prev = inc.get_prev_documents();
        let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
        acro_ref = match cat.get(b"AcroForm") {
            Ok(Object::Reference(id)) => Some(*id),
            Ok(Object::Dictionary(_)) => None, // inline in catalog
            _ => return attach_new_acroform(inc, field_ids, dr_additions), // absent
        };
        let acro = match acro_ref {
            Some(id) => prev.get_dictionary(id).map_err(|e| e.to_string())?,
            None => cat
                .get(b"AcroForm")
                .and_then(Object::as_dict)
                .map_err(|e| e.to_string())?,
        };
        fields_indirect = match acro.get(b"Fields") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        da_absent = acro.get(b"DA").is_err();
        font_plan = match acro.get(b"DR") {
            Ok(Object::Reference(dr_id)) => {
                // Indirect /DR: does it hold an indirect /Font?
                match prev
                    .get_dictionary(*dr_id)
                    .ok()
                    .and_then(|d| d.get(b"Font").ok())
                    .and_then(|o| o.as_reference().ok())
                {
                    Some(font_id) => FontPlan::Indirect(font_id),
                    None => FontPlan::InlineInDr(*dr_id),
                }
            }
            Ok(Object::Dictionary(dr)) => match dr.get(b"Font") {
                Ok(Object::Reference(font_id)) => FontPlan::Indirect(*font_id),
                _ => FontPlan::InlineInAcro,
            },
            _ => FontPlan::InlineInAcro, // /DR absent
        };
    }

    // Indirect /Fields array: clone the standalone object and append to it.
    if let Some(arr_id) = fields_indirect {
        inc.opt_clone_object_to_new_document(arr_id)
            .map_err(|e| e.to_string())?;
        let arr = inc
            .new_document
            .get_object_mut(arr_id)
            .and_then(Object::as_array_mut)
            .map_err(|e| e.to_string())?;
        for id in field_ids {
            arr.push(Object::Reference(*id));
        }
    }

    // Font aliases into an indirect target (its own object, or inline within an
    // indirect /DR). InlineInAcro is handled with the AcroForm dict below.
    match font_plan {
        FontPlan::Indirect(font_id) => {
            inc.opt_clone_object_to_new_document(font_id)
                .map_err(|e| e.to_string())?;
            let font = inc
                .new_document
                .get_object_mut(font_id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            set_font_aliases(font, dr_additions);
        }
        FontPlan::InlineInDr(dr_id) => {
            inc.opt_clone_object_to_new_document(dr_id)
                .map_err(|e| e.to_string())?;
            let dr = inc
                .new_document
                .get_object_mut(dr_id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            set_font_aliases(ensure_font_dict(dr), dr_additions);
        }
        FontPlan::InlineInAcro => {}
    }

    // AcroForm-dict inline edits: inline /Fields, InlineInAcro /DR/Font, /DA seed.
    let need_acro_edit =
        fields_indirect.is_none() || matches!(font_plan, FontPlan::InlineInAcro) || da_absent;
    if need_acro_edit {
        let acro = acro_dict_mut(inc, root, acro_ref)?;
        if fields_indirect.is_none() {
            match acro.get_mut(b"Fields") {
                Ok(Object::Array(arr)) => {
                    for id in field_ids {
                        arr.push(Object::Reference(*id));
                    }
                }
                _ => acro.set(
                    "Fields",
                    Object::Array(field_ids.iter().map(|id| Object::Reference(*id)).collect()),
                ),
            }
        }
        if matches!(font_plan, FontPlan::InlineInAcro) {
            if !matches!(acro.get(b"DR"), Ok(Object::Dictionary(_))) {
                acro.set("DR", Object::Dictionary(Dictionary::new()));
            }
            let dr = acro
                .get_mut(b"DR")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            set_font_aliases(ensure_font_dict(dr), dr_additions);
        }
        // Only seed a default /DA when the existing form lacks one; never touch
        // an existing /DA or /NeedAppearances (appearances are pre-built).
        if da_absent {
            acro.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        }
    }
    Ok(())
}

/// Get a mutable handle to the existing AcroForm dictionary, cloning whatever
/// object holds it into `new_document` first (the AcroForm object when indirect,
/// else the catalog when the AcroForm is inline).
fn acro_dict_mut(
    inc: &mut IncrementalDocument,
    root: ObjectId,
    acro_ref: Option<ObjectId>,
) -> Result<&mut Dictionary, String> {
    match acro_ref {
        Some(acro_id) => {
            inc.opt_clone_object_to_new_document(acro_id)
                .map_err(|e| e.to_string())?;
            inc.new_document
                .get_object_mut(acro_id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())
        }
        None => {
            inc.opt_clone_object_to_new_document(root)
                .map_err(|e| e.to_string())?;
            let cat = inc
                .new_document
                .get_object_mut(root)
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            cat.get_mut(b"AcroForm")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())
        }
    }
}

/// Get-or-create the inline `/Font` sub-dictionary of a `/DR` dict.
fn ensure_font_dict(dr: &mut Dictionary) -> &mut Dictionary {
    if !matches!(dr.get(b"Font"), Ok(Object::Dictionary(_))) {
        dr.set("Font", Object::Dictionary(Dictionary::new()));
    }
    dr.get_mut(b"Font")
        .and_then(Object::as_dict_mut)
        .expect("just ensured /Font is a dict")
}

/// Write new `(alias, ref)` font entries. Aliases are already uniquified against
/// the existing `/DR/Font` keys, so this only ever adds.
fn set_font_aliases(font: &mut Dictionary, dr_additions: &[(String, ObjectId)]) {
    for (alias, id) in dr_additions {
        font.set(alias.as_bytes().to_vec(), Object::Reference(*id));
    }
}

/// No-AcroForm path: create a fresh `/AcroForm` and attach it to the catalog.
fn attach_new_acroform(
    inc: &mut IncrementalDocument,
    field_ids: &[ObjectId],
    dr_additions: &[(String, ObjectId)],
) -> Result<(), String> {
    let mut dr_fonts = Dictionary::new();
    for (alias, id) in dr_additions {
        dr_fonts.set(alias.as_bytes().to_vec(), Object::Reference(*id));
    }
    let acro = dictionary! {
        "Fields" => Object::Array(field_ids.iter().map(|id| Object::Reference(*id)).collect()),
        "DR" => Object::Dictionary(dictionary! { "Font" => Object::Dictionary(dr_fonts) }),
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        "NeedAppearances" => Object::Boolean(false),
    };
    let acro_id = inc.new_document.add_object(Object::Dictionary(acro));
    // Attach to the catalog: clone the catalog into new_document and set /AcroForm.
    let root = inc
        .get_prev_documents()
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;
    inc.opt_clone_object_to_new_document(root)
        .map_err(|e| e.to_string())?;
    let cat = inc
        .new_document
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    cat.set("AcroForm", Object::Reference(acro_id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn top_field_names(bytes: &[u8]) -> std::collections::BTreeSet<String> {
        let doc = Document::load_mem(bytes).unwrap();
        crate::inject::test_field_names(&doc)
    }

    #[test]
    fn merges_into_existing_acroform_preserving_fields() {
        let before = top_field_names(FICHA);
        assert!(!before.is_empty(), "fixture must already have fields");
        let fields = r#"[{"type":"text","name":"bpf_new_field","page":0,"x":10,"y":10,"width":80,"height":18}]"#;
        let out = inject_fields_json(FICHA, fields, &[], "[]", false).unwrap();
        let after = top_field_names(&out);
        // Every pre-existing field survives, and our new one is present.
        for name in &before {
            assert!(after.contains(name), "lost field {name}");
        }
        assert!(after.contains("bpf_new_field"));
        // Structural sanity: merged output re-parses and keeps its pages. The
        // stronger qpdf --check gate runs in the TS integration suite / CI.
        let doc = Document::load_mem(&out).unwrap();
        assert!(!doc.get_pages().is_empty());
    }

    #[test]
    fn rejects_name_collision_with_existing_field() {
        let existing = top_field_names(FICHA).into_iter().next().unwrap();
        let fields = format!(
            r#"[{{"type":"text","name":"{existing}","page":0,"x":1,"y":1,"width":10,"height":10}}]"#
        );
        let err = inject_fields_json(FICHA, &fields, &[], "[]", false).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn injects_all_field_types() {
        let base = blank_page_pdf();
        let fields = r#"[
            {"type":"text","name":"txt","page":0,"x":10,"y":10,"width":100,"height":20},
            {"type":"text","name":"ml","page":0,"x":10,"y":40,"width":100,"height":40,"multiline":true},
            {"type":"checkBox","name":"cb","page":0,"x":10,"y":90,"size":12},
            {"type":"radioGroup","name":"rg","options":[{"value":"a","page":0,"x":10,"y":110,"size":12},{"value":"b","page":0,"x":40,"y":110,"size":12}]},
            {"type":"choice","name":"dd","page":0,"x":10,"y":140,"width":100,"height":20,"options":["x","y"],"combo":true},
            {"type":"choice","name":"lb","page":0,"x":10,"y":170,"width":100,"height":40,"options":["x","y"],"combo":false},
            {"type":"signature","name":"sig","page":0,"x":10,"y":220,"width":100,"height":40}
        ]"#;
        let out = inject_fields_json(&base, fields, &[], "[]", false).unwrap();
        let names = top_field_names(&out);
        for n in ["txt", "ml", "cb", "rg", "dd", "lb", "sig"] {
            assert!(names.contains(n), "missing field {n}");
        }
    }

    /// Build a 1-page PDF with NO form via the create path (empty fields).
    fn blank_page_pdf() -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            &[],
            "[]",
            "[]",
            false,
            false,
        )
        .unwrap()
    }

    #[test]
    fn injects_text_field_and_creates_acroform() {
        let base = blank_page_pdf();
        // Sanity: the base has no AcroForm.
        let base_doc = Document::load_mem(&base).unwrap();
        assert!(!base_doc.catalog().unwrap().has(b"AcroForm"));

        let fields = r#"[{"type":"text","name":"total","page":0,"x":10,"y":10,"width":100,"height":20,"value":"hi"}]"#;
        let out = inject_fields_json(&base, fields, &[], "[]", false).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"AcroForm"), "AcroForm must be created");
        // /AcroForm/Fields has exactly our one field.
        let acro = match cat.get(b"AcroForm").unwrap() {
            lopdf::Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            lopdf::Object::Dictionary(d) => d,
            _ => panic!("bad AcroForm"),
        };
        let fields_arr = acro.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields_arr.len(), 1);
        // The widget landed on page 0's /Annots.
        let pages = doc.get_pages();
        let (_, page0) = pages.into_iter().min_by_key(|(n, _)| *n).unwrap();
        let page = doc.get_dictionary(page0).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert!(!annots.is_empty(), "widget must be on page /Annots");
    }

    #[test]
    fn rejects_bad_page_index() {
        let base = blank_page_pdf();
        let fields = r#"[{"type":"text","name":"t","page":5,"x":1,"y":1,"width":10,"height":10}]"#;
        let err = inject_fields_json(&base, fields, &[], "[]", false).unwrap_err();
        assert!(
            err.contains("page"),
            "expected page-range error, got: {err}"
        );
    }

    #[test]
    fn rejects_radio_option_page_out_of_range() {
        // A NON-first radio option references an out-of-range page. The pre-check
        // must reject this cleanly rather than letting build_one_field index
        // page_ids[opt.page] out of bounds (a WASM-boundary panic).
        let base = blank_page_pdf();
        let fields = r#"[{"type":"radioGroup","name":"r","options":[
            {"value":"a","page":0,"x":1,"y":1,"size":10},
            {"value":"b","page":9,"x":1,"y":30,"size":10}
        ]}]"#;
        let err = inject_fields_json(&base, fields, &[], "[]", false).unwrap_err();
        assert!(
            err.contains("page") || err.contains("out of range"),
            "expected page-range error, got: {err}"
        );
    }

    #[test]
    fn rejects_empty_options_radio_group() {
        let base = blank_page_pdf();
        let fields = r#"[{"type":"radioGroup","name":"r","options":[]}]"#;
        let err = inject_fields_json(&base, fields, &[], "[]", false).unwrap_err();
        assert!(
            err.contains("option"),
            "expected empty-options error, got: {err}"
        );
    }
}
