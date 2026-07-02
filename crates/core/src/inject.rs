//! Inject builder-defined AcroForm fields into an already-loaded PDF via an
//! incremental update. This module implements the **no-AcroForm** path: when the
//! target document has no `/AcroForm`, a fresh one is created and attached to the
//! catalog. Merging into an existing `/AcroForm` is a separate slice.

use crate::create::{BuiltField, FieldDef, FieldFont, build_one_field, da_font_alias};
use crate::doc_io::load_pdf;
use crate::draw::{append_annot_to_page, font_dict};
use crate::fonts::{BuiltFont, EmbeddedFontInput, build_embedded_font};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, dictionary};
use std::collections::{BTreeSet, HashMap};

/// Font descriptor for the embedded-font blob (mirror of create.rs FontDesc).
#[derive(serde::Deserialize)]
struct FontDesc {
    offset: usize,
    length: usize,
    #[serde(default = "default_true")]
    subset: bool,
}

fn default_true() -> bool {
    true
}

pub fn inject_fields_json(
    data: &[u8],
    fields_json: &str,
    fonts: &[u8],
    fonts_json: &str,
) -> Result<Vec<u8>, String> {
    let fields: Vec<FieldDef> =
        serde_json::from_str(fields_json).map_err(|e| format!("invalid fields JSON: {e}"))?;
    if fields.is_empty() {
        return Ok(data.to_vec());
    }
    let font_descs: Vec<FontDesc> =
        serde_json::from_str(fonts_json).map_err(|e| format!("invalid fonts JSON: {e}"))?;

    // Validate font descriptor byte ranges and per-field font references up
    // front, mirroring the create path, so a bad request fails before any object
    // is written and the later indexing/`unwrap`s cannot panic.
    for fd in &font_descs {
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        if end > fonts.len() {
            return Err("font range out of bounds".to_string());
        }
    }
    validate_field_fonts(&fields, &font_descs)?;

    let doc = load_pdf(data)?;

    // Collision check against existing top-level field names BEFORE mutating.
    let existing_names = existing_field_names(&doc)?;
    for f in &fields {
        let name = field_name(f);
        if existing_names.contains(name) {
            return Err(format!("field name '{name}' already exists in this document"));
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
    for f in &fields {
        let pg = field_page(f);
        if pg >= page_ids.len() {
            return Err(format!("field page index {pg} out of range"));
        }
    }

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
        let font = field_font(f, &std_aliases, &emb_aliases, &embedded_fonts, &font_descs, fonts);
        let built: BuiltField = build_one_field(&mut inc.new_document, f, &page_ids, font)?;
        acro_field_ids.push(built.top_field_id);
        for (page_idx, widget_id) in built.widgets {
            // Clone the target page into new_document before mutating its /Annots.
            inc.opt_clone_object_to_new_document(page_ids[page_idx])
                .map_err(|e| e.to_string())?;
            append_annot_to_page(&mut inc, page_ids[page_idx], widget_id)?;
        }
    }

    // Attach a brand-new AcroForm (no-AcroForm path); the merge path is separate.
    attach_new_acroform(&mut inc, &acro_field_ids, &dr_additions)?;

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Validate each field's font reference so building cannot panic: embedded
/// `font_id` must be in range (and not combined with comb), and standard-14
/// field fonts must resolve to both an alias and a width table.
fn validate_field_fonts(fields: &[FieldDef], font_descs: &[FontDesc]) -> Result<(), String> {
    for f in fields {
        match f {
            FieldDef::Text {
                font_id: Some(i),
                comb,
                ..
            } => {
                if *i >= font_descs.len() {
                    return Err(format!("font id {i} out of range"));
                }
                if *comb {
                    return Err(
                        "embedded fonts are supported on plain and multiline text fields only"
                            .to_string(),
                    );
                }
            }
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

/// The page index used for the up-front range check. Radio groups have per-kid
/// pages (wired from `BuiltField.widgets`); here we use the first option's page.
fn field_page(f: &FieldDef) -> usize {
    match f {
        FieldDef::Text { page, .. }
        | FieldDef::CheckBox { page, .. }
        | FieldDef::Choice { page, .. }
        | FieldDef::Signature { page, .. } => *page,
        FieldDef::RadioGroup { options, .. } => options.first().map(|o| o.page).unwrap_or(0),
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

    /// Build a 1-page PDF with NO form via the create path (empty fields).
    fn blank_page_pdf() -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            &[],
            "[]",
            "[]",
        )
        .unwrap()
    }

    #[test]
    fn injects_text_field_and_creates_acroform() {
        let base = blank_page_pdf();
        // Sanity: the base has no AcroForm.
        let base_doc = Document::load_mem(&base).unwrap();
        assert!(!base_doc.catalog().unwrap().has(b"AcroForm"));

        let fields =
            r#"[{"type":"text","name":"total","page":0,"x":10,"y":10,"width":100,"height":20,"value":"hi"}]"#;
        let out = inject_fields_json(&base, fields, &[], "[]").unwrap();

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
        let err = inject_fields_json(&base, fields, &[], "[]").unwrap_err();
        assert!(err.contains("page"), "expected page-range error, got: {err}");
    }
}
