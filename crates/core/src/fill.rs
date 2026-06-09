//! Fill engine: apply {name,value} ops to a PDF and incrementally save.

use crate::forms::{self};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId};
use serde::Deserialize;

#[derive(Deserialize)]
struct FillOp {
    name: String,
    value: String,
}

/// Apply the given fill ops to `data` and return new PDF bytes (incremental save).
pub fn fill_fields_json(data: &[u8], ops_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<FillOp> = serde_json::from_str(ops_json).map_err(|e| e.to_string())?;
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;

    // Resolve every op against the immutable doc first, so we can move `doc`
    // into the IncrementalDocument afterwards.
    let mut plan: Vec<Resolved> = Vec::with_capacity(ops.len());
    for op in &ops {
        plan.push(resolve(&doc, op)?);
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    for r in &plan {
        apply(&mut inc, r)?;
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

enum Apply {
    /// Set /V to a string literal.
    Text(String),
    /// Set /V to a string literal and, if matched, /I to [index].
    Dropdown { value: String, index: Option<i64> },
    /// Set group /V to a Name, and each widget's /AS (on-state name or "Off").
    Button { value: String, widgets: Vec<(ObjectId, bool)> },
}

/// Locate the field for `op.name`, classify it, and build the mutation plan.
fn resolve(doc: &Document, op: &FillOp) -> Result<Resolved, String> {
    let (field_id, dict) = find_field(doc, &op.name)
        .ok_or_else(|| format!("no such field: {}", op.name))?;
    let ft = forms::inherited_name(doc, dict, b"FT").unwrap_or_default();
    let ff = forms::inherited_int(doc, dict, b"Ff").unwrap_or(0);
    let kind = forms::classify(&ft, ff);

    let apply = match kind {
        "text" => Apply::Text(op.value.clone()),
        "checkbox" | "radio" => {
            let widgets = button_widgets(doc, field_id, dict, &op.value)?;
            Apply::Button { value: op.value.clone(), widgets }
        }
        "dropdown" | "listbox" => {
            let index = dropdown_index(dict, &op.value);
            if op.value != "Off" && index.is_none() && has_opt(dict) {
                return Err(format!("'{}' is not a valid option for {}", op.value, op.name));
            }
            Apply::Dropdown { value: op.value.clone(), index }
        }
        other => return Err(format!("cannot fill field {} of type {}", op.name, other)),
    };
    Ok(Resolved { field_id, apply })
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
    let targets: Vec<ObjectId> = if kid_ids.is_empty() { vec![field_id] } else { kid_ids };

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
        return Err(format!("'{}' is not a valid on-state for this button", value));
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
    dict.get(b"Opt").and_then(|o| o.as_array()).map(|a| !a.is_empty()).unwrap_or(false)
}

/// Index of `value` within /Opt (matching export value), if present.
fn dropdown_index(dict: &Dictionary, value: &str) -> Option<i64> {
    let arr = dict.get(b"Opt").ok()?.as_array().ok()?;
    arr.iter().position(|o| forms::opt_export(o) == value).map(|i| i as i64)
}

/// Walk /AcroForm/Fields (and /Kids) to find the field whose fully-qualified
/// name equals `name`. Only reference-addressable fields are considered.
fn find_field<'a>(doc: &'a Document, name: &str) -> Option<(ObjectId, &'a Dictionary)> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    let acro = forms::as_dict(doc, catalog.get(b"AcroForm").ok()?).ok()?;
    let entries = acro.get(b"Fields").ok()?.as_array().ok()?;
    let mut stack: Vec<ObjectId> = entries.iter().filter_map(|e| e.as_reference().ok()).collect();
    let mut seen = 0usize;
    while let Some(id) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break; // guard against pathological/cyclic field trees
        }
        let Ok(d) = doc.get_dictionary(id) else { continue };
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
    inc.opt_clone_object_to_new_document(r.field_id).map_err(|e| e.to_string())?;
    match &r.apply {
        Apply::Text(value) => {
            field_dict_mut(inc, r.field_id)?.set("V", Object::string_literal(value.as_str()));
        }
        Apply::Dropdown { value, index } => {
            let d = field_dict_mut(inc, r.field_id)?;
            d.set("V", Object::string_literal(value.as_str()));
            match index {
                Some(i) => { d.set("I", Object::Array(vec![Object::Integer(*i)])); }
                None => { d.remove(b"I"); }
            }
        }
        Apply::Button { value, widgets } => {
            field_dict_mut(inc, r.field_id)?
                .set("V", Object::Name(value.as_bytes().to_vec()));
            for (wid, has) in widgets {
                inc.opt_clone_object_to_new_document(*wid).map_err(|e| e.to_string())?;
                let as_state = if value != "Off" && *has { value.as_str() } else { "Off" };
                field_dict_mut(inc, *wid)?
                    .set("AS", Object::Name(as_state.as_bytes().to_vec()));
            }
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

#[cfg(test)]
mod tests {
    use super::fill_fields_json;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn reparse_value(bytes: &[u8], field_name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array().unwrap().iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["value"].as_str().map(|s| s.to_string()))
    }

    #[test]
    fn fills_text_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA, IGNACIO"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        // Append-only: output starts with the original bytes.
        assert!(out.len() > FICHA.len());
        assert_eq!(&out[..FICHA.len()], FICHA);
        // Re-parse via the public reader.
        assert_eq!(reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(), Some("GARCIA, IGNACIO"));
        // And it is still a loadable PDF.
        Document::load_mem(&out).unwrap();
    }

    fn reparse_field(bytes: &[u8]) -> serde_json::Value {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn fills_radio_group() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        assert_eq!(reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(), Some("Titular"));
    }

    #[test]
    fn fills_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","value":"Casado"}]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        assert_eq!(reparse_value(&out, "beneficiario.estado_civil").as_deref(), Some("Casado"));
    }

    #[test]
    fn rejects_unknown_field() {
        let ops = r#"[{"name":"does.not.exist","value":"x"}]"#;
        let err = fill_fields_json(FICHA, ops).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_radio_state() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Nope"}]"#;
        let err = fill_fields_json(FICHA, ops).unwrap_err();
        assert!(err.contains("on-state"), "got: {err}");
    }

    #[test]
    fn applies_multiple_ops_in_one_save() {
        let ops = r#"[
            {"name":"beneficiario.apellidos_nombres","value":"A"},
            {"name":"beneficiario.tipo_beneficiario","value":"Familiar"}
        ]"#;
        let out = fill_fields_json(FICHA, ops).unwrap();
        let f = reparse_field(&out);
        let by = |n: &str| f.as_array().unwrap().iter().find(|x| x["name"] == n).cloned().unwrap();
        assert_eq!(by("beneficiario.apellidos_nombres")["value"], "A");
        assert_eq!(by("beneficiario.tipo_beneficiario")["value"], "Familiar");
    }
}
