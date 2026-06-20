//! Fill engine: apply {name,value} ops to a PDF and incrementally save.

use crate::appearance;
use crate::forms::{self};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, text_string};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillOp {
    name: String,
    value: Option<String>,
    values: Option<Vec<String>>,
    image_offset: Option<usize>,
    image_length: Option<usize>,
}

/// Apply the given fill ops to `data` and return new PDF bytes (incremental
/// save). `images` is the concatenated image blob the ops' offsets index into.
pub fn fill_fields_json(data: &[u8], ops_json: &str, images: &[u8]) -> Result<Vec<u8>, String> {
    let ops: Vec<FillOp> = serde_json::from_str(ops_json).map_err(|e| e.to_string())?;
    let doc = crate::doc_io::load_pdf(data)?;
    if forms::has_xfa(&doc) {
        return Err(
            "XFA form detected: filling is not supported because viewers render the XFA data, not the AcroForm values"
                .to_string(),
        );
    }

    // Resolve every op against the immutable doc first, so we can move `doc`
    // into the IncrementalDocument afterwards.
    let mut plan: Vec<Resolved> = Vec::with_capacity(ops.len());
    for op in &ops {
        plan.push(resolve(&doc, op, images)?);
    }

    let touched_appearance = plan.iter().any(|r| {
        matches!(
            r.apply,
            Apply::Text { .. }
                | Apply::Dropdown { .. }
                | Apply::ListBoxMulti { .. }
                | Apply::Signature { .. }
        )
    });

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    for r in &plan {
        apply(&mut inc, r)?;
    }
    if touched_appearance {
        clear_need_appearances(&mut inc)?;
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// What to do to one field, pre-computed from the immutable document.
struct Resolved {
    field_id: ObjectId,
    apply: Apply,
}

/// A widget to draw an appearance on: its id and its /Rect [x0 y0 x1 y1].
struct WidgetBox {
    id: ObjectId,
    rect: [f32; 4],
}

/// Per-field appearance inputs shared by text and choice fields.
struct ApInputs {
    da: appearance::Da,
    q: i64,
    font_ref: ObjectId,
    font: String,
    widths: appearance::FontWidths,
    widgets: Vec<WidgetBox>,
    /// True for text-area fields (Ff Multiline bit); choice fields are always false.
    multiline: bool,
}

enum Apply {
    /// Set /V to a string literal and draw an appearance on each widget.
    Text { value: String, ap: ApInputs },
    /// Set /V (+ /I if matched) and draw an appearance on each widget.
    Dropdown {
        value: String,
        index: Option<i64>,
        ap: ApInputs,
    },
    /// Set group /V to a Name, and each widget's /AS (on-state name or "Off").
    Button {
        value: String,
        widgets: Vec<(ObjectId, bool)>,
    },
    /// Draw a visual-only signature image appearance on each widget.
    Signature {
        image: appearance::SignatureImage,
        widgets: Vec<WidgetBox>,
    },
    /// Set /V to an array of strings and /I to the sorted array of indices,
    /// then draw a multi-row highlight appearance on each widget.
    ListBoxMulti {
        values: Vec<String>,
        indices: Vec<i64>,
        options: Vec<String>,
        ap: ApInputs,
    },
}

/// Locate the field for `op.name`, classify it, and build the mutation plan.
fn resolve(doc: &Document, op: &FillOp, images: &[u8]) -> Result<Resolved, String> {
    let (field_id, dict) =
        find_field(doc, &op.name).ok_or_else(|| format!("no such field: {}", op.name))?;
    let ft = forms::inherited_name(doc, dict, b"FT").unwrap_or_default();
    let ff = forms::inherited_int(doc, dict, b"Ff").unwrap_or(0);
    let kind = forms::classify(&ft, ff);

    let image_bytes = match (op.image_offset, op.image_length) {
        (Some(off), Some(len)) => Some(
            off.checked_add(len)
                .and_then(|end| images.get(off..end))
                .ok_or_else(|| format!("image range out of bounds for field {}", op.name))?,
        ),
        (None, None) => None,
        _ => return Err(format!("field {} op has a partial image range", op.name)),
    };
    let apply = if let Some(image) = image_bytes {
        if op.value.is_some() {
            return Err(format!(
                "field {} op cannot contain both value and image",
                op.name
            ));
        }
        if kind != "signature" {
            return Err(format!(
                "cannot set image on field {} of type {}",
                op.name, kind
            ));
        }
        let image = appearance::signature_image(image)?;
        Apply::Signature {
            image,
            widgets: widget_boxes(doc, field_id, dict),
        }
    } else {
        // Multi-value fills are only legal on a multiselect list box.
        if let Some(values) = &op.values {
            if op.value.is_some() {
                return Err(format!(
                    "field {} op cannot contain both value and values",
                    op.name
                ));
            }
            if kind != "listbox" || !forms::is_multiselect(ff) {
                return Err(format!(
                    "field {} does not accept multiple values (not a multi-select list box)",
                    op.name
                ));
            }
            let options: Vec<String> = dict
                .get(b"Opt")
                .and_then(|o| o.as_array())
                .map(|a| a.iter().map(forms::opt_export).collect())
                .unwrap_or_default();
            // Build (index, value) pairs so /V and /I stay positionally aligned
            // after sorting by index (PDF §12.7.4.4 requires /V to match /I order).
            let mut pairs: Vec<(i64, String)> = Vec::with_capacity(values.len());
            for v in values {
                match dropdown_index(dict, v) {
                    Some(i) => pairs.push((i, v.clone())),
                    None => {
                        return Err(format!("'{}' is not a valid option for {}", v, op.name));
                    }
                }
            }
            pairs.sort_unstable_by_key(|(i, _)| *i);
            let (indices, sorted_values): (Vec<i64>, Vec<String>) = pairs.into_iter().unzip();
            return Ok(Resolved {
                field_id,
                apply: Apply::ListBoxMulti {
                    values: sorted_values,
                    indices,
                    options,
                    ap: ap_inputs(doc, field_id, dict, &op.name, ff)?,
                },
            });
        }
        let value = op
            .value
            .as_ref()
            .ok_or_else(|| format!("missing value for field {}", op.name))?;
        match kind {
            "text" => Apply::Text {
                value: value.clone(),
                ap: ap_inputs(doc, field_id, dict, &op.name, ff)?,
            },
            "checkbox" | "radio" => {
                let widgets = button_widgets(doc, field_id, dict, value)?;
                Apply::Button {
                    value: value.clone(),
                    widgets,
                }
            }
            "dropdown" | "listbox" => {
                let index = dropdown_index(dict, value);
                if value != "Off" && index.is_none() && has_opt(dict) {
                    return Err(format!("'{}' is not a valid option for {}", value, op.name));
                }
                Apply::Dropdown {
                    value: value.clone(),
                    index,
                    ap: ap_inputs(doc, field_id, dict, &op.name, ff)?,
                }
            }
            other => return Err(format!("cannot fill field {} of type {}", op.name, other)),
        }
    };
    Ok(Resolved { field_id, apply })
}

/// Gather everything needed to draw a text/choice field's appearance:
/// effective DA, quadding, the DR font reference, and the widget boxes.
fn ap_inputs(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    name: &str,
    ff: i64,
) -> Result<ApInputs, String> {
    let acro = forms::acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
    let da_str = effective_da(doc, dict, acro);
    let da = appearance::parse_da(&da_str);
    let font_ref = font_ref(doc, acro, &da.font)
        .ok_or_else(|| format!("DA font '{}' not found in /DR for {}", da.font, name))?;
    Ok(ApInputs {
        q: quadding(doc, dict),
        font: da.font.clone(),
        widths: resolve_widths(doc, acro, &da.font),
        da,
        font_ref,
        widgets: widget_boxes(doc, field_id, dict),
        multiline: forms::is_multiline(ff),
    })
}

/// Effective /DA: field's own, else inherited, else AcroForm's, else default.
fn effective_da(doc: &Document, dict: &Dictionary, acro: &Dictionary) -> String {
    if let Some(s) = inherited_str(doc, dict, b"DA") {
        return s;
    }
    acro.get(b"DA")
        .ok()
        .and_then(da_string)
        .unwrap_or_else(|| "/Helv 0 Tf 0 g".to_string())
}

/// A string value on the field or any ancestor (for inheritable keys like /DA).
fn inherited_str(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    if let Some(s) = dict.get(key).ok().and_then(da_string) {
        return Some(s);
    }
    let mut cur = dict;
    for _ in 0..forms::MAX_PARENT_DEPTH {
        let parent = forms::parent_of(doc, cur)?;
        if let Some(s) = parent.get(key).ok().and_then(da_string) {
            return Some(s);
        }
        cur = parent;
    }
    None
}

fn da_string(o: &Object) -> Option<String> {
    o.as_str()
        .ok()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// Resolve `font` (from DA) to its indirect object id via AcroForm /DR/Font.
fn font_ref(doc: &Document, acro: &Dictionary, font: &str) -> Option<ObjectId> {
    let dr = forms::as_dict(doc, acro.get(b"DR").ok()?).ok()?;
    let fonts = forms::as_dict(doc, dr.get(b"Font").ok()?).ok()?;
    fonts.get(font.as_bytes()).ok()?.as_reference().ok()
}

/// Collect a field's drawable widgets (id + /Rect). A field with no /Kids is
/// its own widget.
fn widget_boxes(doc: &Document, field_id: ObjectId, dict: &Dictionary) -> Vec<WidgetBox> {
    let ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let ids = if ids.is_empty() { vec![field_id] } else { ids };
    ids.into_iter()
        .filter_map(|id| {
            let d = doc.get_dictionary(id).ok()?;
            let r = d.get(b"Rect").ok()?.as_array().ok()?;
            let mut rect = [0f32; 4];
            for (i, v) in r.iter().enumerate().take(4) {
                rect[i] = v.as_float().unwrap_or(0.0);
            }
            Some(WidgetBox { id, rect })
        })
        .collect()
}

fn quadding(doc: &Document, dict: &Dictionary) -> i64 {
    forms::inherited_int(doc, dict, b"Q").unwrap_or(0)
}

/// The /DR/Font/<name> dictionary for a DA font name, if present.
fn font_dict<'a>(doc: &'a Document, acro: &'a Dictionary, font: &str) -> Option<&'a Dictionary> {
    let dr = forms::as_dict(doc, acro.get(b"DR").ok()?).ok()?;
    let fonts = forms::as_dict(doc, dr.get(b"Font").ok()?).ok()?;
    forms::as_dict(doc, fonts.get(font.as_bytes()).ok()?).ok()
}

/// Width table for the DA font: standard-14 metrics by /BaseFont when
/// recognized, else the font's own /Widths array, else Helvetica.
fn resolve_widths(doc: &Document, acro: &Dictionary, da_font: &str) -> appearance::FontWidths {
    if let Some(fd) = font_dict(doc, acro, da_font) {
        if let Some(base) = fd.get(b"BaseFont").ok().and_then(|o| o.as_name().ok())
            && let Some(w) = appearance::standard_14_widths(&String::from_utf8_lossy(base))
        {
            return w;
        }
        if let Some(w) = widths_from_font_dict(doc, fd) {
            return w;
        }
    }
    appearance::helvetica_widths()
}

/// Build a width table from a simple font's /FirstChar + /Widths entries.
fn widths_from_font_dict(doc: &Document, fd: &Dictionary) -> Option<appearance::FontWidths> {
    let first = fd.get(b"FirstChar").ok()?.as_i64().ok()?;
    let widths_obj = fd.get(b"Widths").ok()?;
    let arr = match widths_obj {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
        Object::Array(a) => a,
        _ => return None,
    };
    let mut table = [0u16; 224];
    for (i, w) in arr.iter().enumerate() {
        let code = first + i as i64;
        if (32..=255).contains(&code) {
            table[(code - 32) as usize] = w.as_float().unwrap_or(0.0).round() as u16;
        }
    }
    Some(appearance::FontWidths(table))
}

/// Resolve the button's widget set and validate the requested on-state.
/// Returns (widget_id, has_target_state) for each widget. A field with no
/// /Kids is its own widget.
fn button_widgets(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    value: &str,
) -> Result<Vec<(ObjectId, bool)>, String> {
    let mut widgets: Vec<(ObjectId, bool)> = Vec::new();
    let kid_ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let targets: Vec<ObjectId> = if kid_ids.is_empty() {
        vec![field_id]
    } else {
        kid_ids
    };

    let mut any_match = false;
    for id in targets {
        let has = doc
            .get_dictionary(id)
            .ok()
            .map(|w| widget_has_state(doc, w, value))
            .unwrap_or(false);
        if has {
            any_match = true;
        }
        widgets.push((id, has));
    }
    if value != "Off" && !any_match {
        return Err(format!(
            "'{}' is not a valid on-state for this button",
            value
        ));
    }
    Ok(widgets)
}

/// True if a widget's /AP/N has a sub-key named `state`.
fn widget_has_state(doc: &Document, widget: &Dictionary, state: &str) -> bool {
    let mut found = Vec::new();
    forms::collect_on_states(doc, widget, &mut found);
    found.iter().any(|s| s == state)
}

fn has_opt(dict: &Dictionary) -> bool {
    dict.get(b"Opt")
        .and_then(|o| o.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

/// Index of `value` within /Opt (matching export value), if present.
fn dropdown_index(dict: &Dictionary, value: &str) -> Option<i64> {
    let arr = dict.get(b"Opt").ok()?.as_array().ok()?;
    arr.iter()
        .position(|o| forms::opt_export(o) == value)
        .map(|i| i as i64)
}

/// Walk /AcroForm/Fields (and /Kids) to find the field whose fully-qualified
/// name equals `name`. Only reference-addressable fields are considered.
pub(crate) fn find_field<'a>(doc: &'a Document, name: &str) -> Option<(ObjectId, &'a Dictionary)> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    let acro = forms::as_dict(doc, catalog.get(b"AcroForm").ok()?).ok()?;
    let entries = acro.get(b"Fields").ok()?.as_array().ok()?;
    let mut stack: Vec<ObjectId> = entries
        .iter()
        .filter_map(|e| e.as_reference().ok())
        .collect();
    let mut seen = 0usize;
    while let Some(id) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break; // guard against pathological/cyclic field trees
        }
        let Ok(d) = doc.get_dictionary(id) else {
            continue;
        };
        if forms::fully_qualified_name(doc, d) == name {
            return Some((id, d));
        }
        if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
            for k in kids {
                if let Ok(kid_id) = k.as_reference() {
                    stack.push(kid_id);
                }
            }
        }
    }
    None
}

/// Apply one resolved mutation onto the incremental document.
fn apply(inc: &mut IncrementalDocument, r: &Resolved) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(r.field_id)
        .map_err(|e| e.to_string())?;
    match &r.apply {
        Apply::Text { value, ap } => {
            field_dict_mut(inc, r.field_id)?.set("V", text_string(value));
            draw_appearances(inc, value, ap)?;
        }
        Apply::Dropdown { value, index, ap } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                d.set("V", text_string(value));
                match index {
                    Some(i) => {
                        d.set("I", Object::Array(vec![Object::Integer(*i)]));
                    }
                    None => {
                        d.remove(b"I");
                    }
                }
            }
            draw_appearances(inc, value, ap)?;
        }
        Apply::Button { value, widgets } => {
            field_dict_mut(inc, r.field_id)?.set("V", Object::Name(value.as_bytes().to_vec()));
            for (wid, has) in widgets {
                inc.opt_clone_object_to_new_document(*wid)
                    .map_err(|e| e.to_string())?;
                let as_state = if value != "Off" && *has {
                    value.as_str()
                } else {
                    "Off"
                };
                field_dict_mut(inc, *wid)?.set("AS", Object::Name(as_state.as_bytes().to_vec()));
            }
        }
        Apply::Signature { image, widgets } => {
            draw_signature_appearances(inc, image, widgets)?;
        }
        Apply::ListBoxMulti {
            values,
            indices,
            options,
            ap,
        } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                let v_arr: Vec<Object> = values.iter().map(|s| text_string(s)).collect();
                d.set("V", Object::Array(v_arr));
                let i_arr: Vec<Object> = indices.iter().map(|i| Object::Integer(*i)).collect();
                d.set("I", Object::Array(i_arr));
            }
            draw_listbox_multi_appearances(inc, options, indices, ap)?;
        }
    }
    Ok(())
}

fn field_dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.new_document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())
}

/// Build and attach a `/AP/N` appearance stream on each of the field's widgets.
fn draw_appearances(
    inc: &mut IncrementalDocument,
    value: &str,
    ap: &ApInputs,
) -> Result<(), String> {
    let text = appearance::encode_winansi(value);
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content = if ap.multiline {
            // Multiline: do not shrink-to-fit width (we wrap instead). Honor an
            // explicit DA size; for auto (size 0) use a fixed, height-clamped
            // default so wrapping has a stable measure.
            let size = if ap.da.size > 0.0 {
                ap.da.size
            } else {
                (h - 2.0).clamp(appearance::MIN_AUTO, appearance::MAX_AUTO)
            };
            let avail_w = (w - 4.0).max(1.0);
            let lines = appearance::wrap_lines(&text, size, avail_w, &ap.widths);
            appearance::text_appearance_content_multiline(
                &lines, size, w, h, ap.q, &ap.da.color, &ap.font, &ap.widths,
            )
        } else {
            let size = appearance::auto_size(ap.da.size, &text, (w - 4.0).max(1.0), h, &ap.widths);
            appearance::text_appearance_content(
                &text,
                size,
                w,
                h,
                ap.q,
                &ap.da.color,
                &ap.font,
                &ap.widths,
            )
        };
        let xobj = appearance::build_appearance_xobject(content, w, h, &ap.font, ap.font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Build and attach a multi-row highlight `/AP/N` on each widget.
fn draw_listbox_multi_appearances(
    inc: &mut IncrementalDocument,
    options: &[String],
    indices: &[i64],
    ap: &ApInputs,
) -> Result<(), String> {
    let encoded: Vec<Vec<u8>> = options.iter().map(|s| appearance::encode_winansi(s)).collect();
    let selected: Vec<bool> = (0..options.len() as i64)
        .map(|i| indices.contains(&i))
        .collect();
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content = appearance::listbox_multi_content(
            &encoded,
            &selected,
            ap.da.size,
            w,
            h,
            &ap.da.color,
            &ap.font,
        );
        let xobj = appearance::build_appearance_xobject(content, w, h, &ap.font, ap.font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Build and attach a visual signature `/AP/N` on each signature widget.
fn draw_signature_appearances(
    inc: &mut IncrementalDocument,
    image: &appearance::SignatureImage,
    widgets: &[WidgetBox],
) -> Result<(), String> {
    let info = image.info();
    let image_id =
        inc.new_document
            .add_object(Object::Stream(appearance::build_signature_image_xobject(
                image.clone(),
            )));

    for wb in widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let xobj = appearance::build_signature_appearance_xobject(
            image_id,
            info.width as f32,
            info.height as f32,
            w,
            h,
        );
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Set /NeedAppearances false on the AcroForm, cloning whatever object holds it
/// (the Catalog if AcroForm is inline, else the AcroForm object itself).
fn clear_need_appearances(inc: &mut IncrementalDocument) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let root = prev
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            field_dict_mut(inc, id)?.set("NeedAppearances", Object::Boolean(false));
        }
        Ok(Object::Dictionary(_)) => {
            inc.opt_clone_object_to_new_document(root)
                .map_err(|e| e.to_string())?;
            let cat = field_dict_mut(inc, root)?;
            let acro = cat
                .get_mut(b"AcroForm")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            acro.set("NeedAppearances", Object::Boolean(false));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fill_fields_json, find_field};
    use lopdf::{Document, Object, ObjectId, StringFormat};

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
    const FICHA_OBJSTREAMS: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-objstreams.pdf");
    const ANEXO: &[u8] = include_bytes!("../../../tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf");
    const FICHA_XFA: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-xfa.pdf");
    const TINY_JPEG: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x02,
        0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
    ];

    fn reparse_value(bytes: &[u8], field_name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["value"].as_str().map(|s| s.to_string()))
    }

    #[test]
    fn fills_text_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA, IGNACIO"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        // Append-only: output starts with the original bytes.
        assert!(out.len() > FICHA.len());
        assert_eq!(&out[..FICHA.len()], FICHA);
        // Re-parse via the public reader.
        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("GARCIA, IGNACIO")
        );
        // And it is still a loadable PDF.
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn fills_accented_text_value_as_pdf_text_string() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"Juan P\u00e9rez"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, field) = find_field(&doc, "beneficiario.apellidos_nombres").unwrap();
        let v = field.get(b"V").unwrap();

        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("Juan P\u{e9}rez")
        );
        match v {
            Object::String(bytes, StringFormat::Hexadecimal) => {
                assert_eq!(
                    bytes,
                    &vec![
                        0xfe, 0xff, 0x00, b'J', 0x00, b'u', 0x00, b'a', 0x00, b'n', 0x00, b' ',
                        0x00, b'P', 0x00, 0xe9, 0x00, b'r', 0x00, b'e', 0x00, b'z',
                    ]
                );
            }
            _ => panic!("expected UTF-16BE hexadecimal text string"),
        }
    }

    fn reparse_field(bytes: &[u8]) -> serde_json::Value {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn fills_radio_group() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        assert_eq!(
            reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(),
            Some("Titular")
        );
    }

    #[test]
    fn fills_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","value":"Casado"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        assert_eq!(
            reparse_value(&out, "beneficiario.estado_civil").as_deref(),
            Some("Casado")
        );
    }

    #[test]
    fn rejects_unknown_field() {
        let ops = r#"[{"name":"does.not.exist","value":"x"}]"#;
        let err = fill_fields_json(FICHA, ops, &[]).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_radio_state() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Nope"}]"#;
        let err = fill_fields_json(FICHA, ops, &[]).unwrap_err();
        assert!(err.contains("on-state"), "got: {err}");
    }

    /// Read a field's /AP/N stream content as a string, if present.
    fn ap_content(doc: &Document, field_name: &str) -> Option<String> {
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        let mut stack: Vec<ObjectId> = acro
            .get(b"Fields")
            .ok()?
            .as_array()
            .ok()?
            .iter()
            .filter_map(|e| e.as_reference().ok())
            .collect();
        while let Some(id) = stack.pop() {
            let Ok(d) = doc.get_dictionary(id) else {
                continue;
            };
            if crate::forms::fully_qualified_name(doc, d) == field_name {
                let n = d
                    .get(b"AP")
                    .ok()?
                    .as_dict()
                    .ok()?
                    .get(b"N")
                    .ok()?
                    .as_reference()
                    .ok()?;
                let st = doc.get_object(n).ok()?.as_stream().ok()?;
                return Some(String::from_utf8_lossy(&st.content).into_owned());
            }
            if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
                for k in kids {
                    if let Ok(r) = k.as_reference() {
                        stack.push(r);
                    }
                }
            }
        }
        None
    }

    fn need_appearances(doc: &Document) -> Option<bool> {
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        acro.get(b"NeedAppearances")
            .ok()
            .and_then(|o| o.as_bool().ok())
    }

    /// Set the Multiline flag (Ff bit 13) on a text field and return the bytes of
    /// the modified document, so we can exercise the multiline fill path on a real
    /// fixture field even though the corpus ships only single-line text fields.
    fn with_multiline_flag(bytes: &[u8], field_name: &str) -> Vec<u8> {
        let mut doc = Document::load_mem(bytes).unwrap();
        let (id, _) = find_field(&doc, field_name).unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        d.set("Ff", Object::Integer(1 << 12));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn multiline_text_fill_wraps_into_multiple_lines() {
        // Confirm the target field is wide-but-short enough to force a wrap. The
        // value is long with spaces so greedy wrapping must break it across lines.
        let base = with_multiline_flag(FICHA, "beneficiario.apellidos_nombres");
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"the quick brown fox jumps over the lazy dog several times to overflow"}]"#;
        let out = fill_fields_json(&base, ops, &[]).unwrap();
        Document::load_mem(&out).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
        assert!(ap.contains("TL"), "multiline AP should set leading: {ap}");
        assert!(
            ap.matches(" Tj").count() >= 2,
            "expected multiple Tj (wrapped lines), got: {ap}"
        );
    }

    #[test]
    fn text_fill_generates_appearance() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
        assert!(ap.contains("(GARCIA) Tj"), "got: {ap}");
        assert!(ap.contains("Tf"));
    }

    #[test]
    fn fill_flips_need_appearances_false() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"X"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(need_appearances(&doc), Some(false));
    }

    #[test]
    fn radio_fill_does_not_add_appearance_stream() {
        // Buttons already have /AP; we must not overwrite with a text stream.
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        Document::load_mem(&out).unwrap(); // still valid
        assert_eq!(
            reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(),
            Some("Titular")
        );
    }

    #[test]
    fn applies_multiple_ops_in_one_save() {
        let ops = r#"[
            {"name":"beneficiario.apellidos_nombres","value":"A"},
            {"name":"beneficiario.tipo_beneficiario","value":"Familiar"}
        ]"#;
        let out = fill_fields_json(FICHA, ops, &[]).unwrap();
        let f = reparse_field(&out);
        let by = |n: &str| {
            f.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == n)
                .cloned()
                .unwrap()
        };
        assert_eq!(by("beneficiario.apellidos_nombres")["value"], "A");
        assert_eq!(by("beneficiario.tipo_beneficiario")["value"], "Familiar");
    }

    #[test]
    fn visual_signature_generates_image_appearance() {
        let ops = r#"[{"name":"firma.titular","imageOffset":0,"imageLength":21}]"#;
        let out = fill_fields_json(ANEXO, ops, TINY_JPEG).unwrap();
        assert!(out.len() > ANEXO.len());
        assert_eq!(&out[..ANEXO.len()], ANEXO);

        Document::load_mem(&out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("/DCTDecode"), "missing JPEG image XObject");
        assert!(
            s.contains("/SigImg Do"),
            "missing signature form appearance draw"
        );
    }

    #[test]
    fn visual_signature_rejects_non_signature_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","imageOffset":0,"imageLength":21}]"#;
        let err = fill_fields_json(FICHA, ops, TINY_JPEG).unwrap_err();
        assert!(err.contains("cannot set image on field"), "got: {err}");
    }

    #[test]
    fn rejects_out_of_bounds_image_range() {
        let ops = r#"[{"name":"firma.titular","imageOffset":10,"imageLength":100}]"#;
        let err = fill_fields_json(ANEXO, ops, TINY_JPEG).unwrap_err();
        assert!(err.contains("image range"), "got: {err}");
    }

    #[test]
    fn rejects_xfa_forms_on_fill() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"x"}]"#;
        let err = fill_fields_json(FICHA_XFA, ops, &[]).unwrap_err();
        assert!(err.contains("XFA"), "got: {err}");
    }

    #[test]
    fn reads_widths_array_from_font_dict() {
        use lopdf::{Dictionary, Document, Object};
        let mut fd = Dictionary::new();
        fd.set("FirstChar", Object::Integer(65));
        fd.set(
            "Widths",
            Object::Array(vec![Object::Integer(500), Object::Real(750.0)]),
        );
        let doc = Document::with_version("1.3");
        let w = super::widths_from_font_dict(&doc, &fd).unwrap();
        assert_eq!(w.width(b'A'), 500);
        assert_eq!(w.width(b'B'), 750);
        assert_eq!(w.width(b'C'), 556); // default for unset codes
    }

    /// Load FICHA, set the Multiselect Ff bit on `field_name` (clearing Combo bit), return new bytes.
    fn with_multiselect(bytes: &[u8], field_name: &str) -> Vec<u8> {
        use lopdf::Document;
        let mut doc = Document::load_mem(bytes).unwrap();
        let (id, _) = find_field(&doc, field_name).unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
        // Clear the Combo flag (bit 18, 1<<17) and set Multiselect (bit 22, 1<<21).
        d.set("Ff", Object::Integer((ff & !(1 << 17)) | (1 << 21)));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn multiselect_fill_sets_v_array_and_sorted_i() {
        use lopdf::Document;
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        // Provide values out of /Opt order; expect /I sorted ascending.
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
        let out = fill_fields_json(&base, ops, &[]).unwrap();
        // Check /V is an Array of 2 entries and /I == [1, 3].
        let doc = Document::load_mem(&out).unwrap();
        let (_, field) = find_field(&doc, "beneficiario.estado_civil").unwrap();
        let v_arr = field.get(b"V").unwrap().as_array().unwrap();
        assert_eq!(v_arr.len(), 2, "/V must be an array of 2 strings");
        let i_arr = field.get(b"I").unwrap().as_array().unwrap();
        let i: Vec<i64> = i_arr.iter().map(|o| o.as_i64().unwrap()).collect();
        // "Casado" is index 1, "Viudo" is index 3 in /Opt -> sorted [1, 3].
        assert_eq!(i, vec![1, 3]);
        // /V must be sorted by index too: Casado(1) before Viudo(3).
        let v0 = lopdf::decode_text_string(&v_arr[0]).unwrap();
        let v1 = lopdf::decode_text_string(&v_arr[1]).unwrap();
        assert_eq!(v0, "Casado", "/V[0] must be Casado (index 1)");
        assert_eq!(v1, "Viudo", "/V[1] must be Viudo (index 3)");
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn rejects_multivalue_on_single_select_listbox() {
        // estado_civil WITHOUT the Multiselect flag set - first need to make it a listbox
        // by clearing the Combo bit, but without setting Multiselect.
        use lopdf::Document;
        let mut doc = Document::load_mem(FICHA).unwrap();
        let (id, _) = find_field(&doc, "beneficiario.estado_civil").unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
        // Clear Combo bit only -> becomes a single-select listbox
        d.set("Ff", Object::Integer(ff & !(1 << 17)));
        let mut base = Vec::new();
        doc.save_to(&mut base).unwrap();

        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Viudo"]}]"#;
        let err = fill_fields_json(&base, ops, &[]).unwrap_err();
        assert!(err.contains("does not accept multiple values"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_option_in_multivalue_fill() {
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Nope"]}]"#;
        let err = fill_fields_json(&base, ops, &[]).unwrap_err();
        assert!(err.contains("not a valid option"), "got: {err}");
    }

    #[test]
    fn multiselect_fill_generates_highlight_appearance() {
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
        let out = fill_fields_json(&base, ops, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.estado_civil").expect("AP/N present");
        assert!(ap.contains("0.60 0.75 0.85 rg"), "no highlight: {ap}");
        assert_eq!(ap.matches(" re").count(), 2, "expected 2 highlights: {ap}");
        assert!(ap.contains("(Casado) Tj"), "missing option text: {ap}");
    }

    #[test]
    fn fills_xref_stream_pdf_incrementally() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA"}]"#;
        let out = fill_fields_json(FICHA_OBJSTREAMS, ops, &[]).unwrap();
        // Still append-only.
        assert_eq!(&out[..FICHA_OBJSTREAMS.len()], FICHA_OBJSTREAMS);
        // Re-parses with the new value.
        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("GARCIA")
        );
        Document::load_mem(&out).unwrap();
    }
}
