use crate::flatten::field_widgets;
use lopdf::{Dictionary, Document, Object, ObjectId, decode_text_string};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct FieldInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub value: Option<String>,
    /// The field's default/reset value (`/DV`), or `null` when it has none.
    #[serde(rename = "defaultValue")]
    pub default_value: Option<String>,
    pub states: Vec<String>,
    pub options: Vec<String>,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
    pub required: bool,
    /// True unless the field's `NoExport` flag is set.
    pub exported: bool,
    /// Text field `/MaxLen`, if declared; `null` for other fields or when unset.
    #[serde(rename = "maxLength")]
    pub max_length: Option<u32>,
    /// True only for multi-select list boxes (the PDF Multiselect choice flag).
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    /// True only for password text fields (the PDF Password text flag): the
    /// value should be masked rather than displayed.
    pub password: bool,
    /// True only for multi-line text fields (the PDF Multiline text flag).
    pub multiline: bool,
    /// True only for comb text fields (the PDF Comb text flag): a single line
    /// split into `maxLength` fixed-pitch per-character cells.
    pub comb: bool,
    /// True only for editable dropdowns (the PDF combo box Edit flag): the user
    /// may type a value that is not one of `options`.
    pub editable: bool,
    /// Horizontal alignment of the field's text, from `/Q`. One of `"left"`,
    /// `"center"`, or `"right"`; defaults to `"left"` when undeclared.
    pub align: &'static str,
    /// The field's tooltip / alternate descriptive name (`/TU`), or `null` when
    /// the field has none.
    pub tooltip: Option<String>,
    /// For variable-text fields (text/dropdown/listbox), the font resource name
    /// from the effective `/DA` (e.g. `"Helv"`), or `null` for other field types
    /// or when no `/DA` applies.
    #[serde(rename = "fontName")]
    pub font_name: Option<String>,
    /// For variable-text fields, the font size in points from the effective
    /// `/DA`. `0` means auto-size (the PDF `0 Tf` convention); `null` for other
    /// field types or when no `/DA` applies.
    #[serde(rename = "fontSize")]
    pub font_size: Option<f32>,
    /// One entry per widget annotation: its page index (0-based) and `/Rect`
    /// `[x0, y0, x1, y1]` in PDF points (origin bottom-left). Most fields have
    /// one; radio groups and fields repeated across pages have several.
    pub widgets: Vec<Widget>,
}

#[derive(Serialize)]
pub struct Widget {
    pub page: usize,
    pub rect: [f32; 4],
    /// Annotation `/F` Hidden flag (bit 2): not displayed and not printed.
    pub hidden: bool,
    /// Annotation `/F` Print flag (bit 3): included when the page is printed.
    pub print: bool,
    /// Annotation `/F` NoView flag (bit 6): hidden on screen but may still print.
    #[serde(rename = "noView")]
    pub no_view: bool,
}

/// Parse `data` and return its AcroForm fields as a JSON array string.
pub fn read_fields_json(data: &[u8]) -> Result<String, String> {
    let doc = crate::doc_io::load_pdf(data)?;
    let fields = collect_fields(&doc).map_err(|e| e.to_string())?;
    serde_json::to_string(&fields).map_err(|e| e.to_string())
}

fn collect_fields(doc: &Document) -> Result<Vec<FieldInfo>, String> {
    let catalog = as_dict(doc, doc.trailer.get(b"Root").map_err(|e| e.to_string())?)?;
    let acroform = match catalog.get(b"AcroForm") {
        Ok(o) => as_dict(doc, o)?,
        Err(_) => return Ok(Vec::new()),
    };
    let entries = acroform
        .get(b"Fields")
        .and_then(|o| o.as_array())
        .map_err(|e| e.to_string())?;
    let pages = page_index_map(doc);
    let mut out = Vec::new();
    for entry in entries {
        let id = entry.as_reference().ok();
        let d = as_dict(doc, entry)?;
        out.push(describe_field(doc, id, d, &pages));
    }
    Ok(out)
}

/// Map each page's object id to its 0-based index, in page order.
fn page_index_map(doc: &Document) -> HashMap<ObjectId, usize> {
    doc.get_pages()
        .values()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect()
}

fn describe_field(
    doc: &Document,
    field_id: Option<ObjectId>,
    d: &Dictionary,
    pages: &HashMap<ObjectId, usize>,
) -> FieldInfo {
    let name = fully_qualified_name(doc, d);
    let ft = inherited_name(doc, d, b"FT").unwrap_or_default();
    let ff = inherited_int(doc, d, b"Ff").unwrap_or(0);
    let field_type = classify(&ft, ff).to_string();
    let value = field_value(d, b"V");
    let default_value = field_value(d, b"DV");

    let mut states = Vec::new();
    collect_on_states(doc, d, &mut states);
    if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kd) = as_dict(doc, k) {
                collect_on_states(doc, kd, &mut states);
            }
        }
    }

    let options = d
        .get(b"Opt")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().map(opt_export).collect())
        .unwrap_or_default();

    // `/MaxLen` is a text-field property; ignore it for other field types.
    let max_length = if field_type == "text" {
        inherited_int(doc, d, b"MaxLen")
            .filter(|&n| n >= 0)
            .map(|n| n as u32)
    } else {
        None
    };

    // Font/size come from the effective /DA; only meaningful for variable-text
    // fields (text and choice). Other field types report null.
    let (font_name, font_size) = if matches!(field_type.as_str(), "text" | "dropdown" | "listbox") {
        match effective_da(doc, d) {
            Some(s) => {
                let da = crate::appearance::parse_da(&s);
                (Some(da.font), Some(da.size))
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };

    let widgets = field_id
        .map(|id| {
            field_widgets(doc, id, d)
                .into_iter()
                .filter_map(|w| {
                    pages.get(&w.page_id).map(|&page| {
                        let f = doc
                            .get_dictionary(w.id)
                            .ok()
                            .and_then(|wd| wd.get(b"F").ok())
                            .and_then(|o| o.as_i64().ok())
                            .unwrap_or(0);
                        Widget {
                            page,
                            rect: w.rect,
                            hidden: f & 2 != 0,
                            print: f & 4 != 0,
                            no_view: f & 32 != 0,
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    FieldInfo {
        name,
        field_type: field_type.clone(),
        value,
        default_value,
        states,
        options,
        read_only: ff & 1 != 0,
        required: ff & 2 != 0,
        exported: ff & 4 == 0,
        max_length,
        multi_select: field_type == "listbox" && is_multiselect(ff),
        password: field_type == "text" && is_password(ff),
        multiline: field_type == "text" && is_multiline(ff),
        comb: field_type == "text" && is_comb(ff),
        editable: field_type == "dropdown" && is_combo_edit(ff),
        align: quadding_to_align(inherited_int(doc, d, b"Q").unwrap_or(0)),
        tooltip: d.get(b"TU").ok().and_then(value_to_string),
        font_name,
        font_size,
        widgets,
    }
}

pub(crate) fn classify(ft: &str, ff: i64) -> &'static str {
    match ft {
        "Tx" => "text",
        "Btn" => {
            if ff & (1 << 16) != 0 {
                "pushbutton"
            } else if ff & (1 << 15) != 0 {
                "radio"
            } else {
                "checkbox"
            }
        }
        "Ch" => {
            if ff & (1 << 17) != 0 {
                "dropdown"
            } else {
                "listbox"
            }
        }
        "Sig" => "signature",
        _ => "unknown",
    }
}

/// True when a text field's Ff carries the Multiline flag (bit 13, `1 << 12`),
/// i.e. it is a text-area field that should render wrapped, multi-line text.
pub(crate) fn is_multiline(ff: i64) -> bool {
    ff & (1 << 12) != 0
}

/// True when a text field carries the Password flag (Ff bit 14, `1 << 13`):
/// the value should be masked rather than shown.
pub(crate) fn is_password(ff: i64) -> bool {
    ff & (1 << 13) != 0
}

/// True when a text field carries the Comb flag (Ff bit 25, `1 << 24`): the
/// value is laid out as fixed-pitch per-character cells.
pub(crate) fn is_comb(ff: i64) -> bool {
    ff & (1 << 24) != 0
}

/// True when a choice field carries the Multiselect flag (Ff bit 22).
pub(crate) fn is_multiselect(ff: i64) -> bool {
    ff & (1 << 21) != 0
}

/// True when a choice field carries the combo box Edit flag (Ff bit 19,
/// `1 << 18`): the user may type a value not present in the option list.
pub(crate) fn is_combo_edit(ff: i64) -> bool {
    ff & (1 << 18) != 0
}

/// Map a `/Q` quadding value to an alignment keyword: 1 = center, 2 = right,
/// anything else (including the 0 default) = left.
pub(crate) fn quadding_to_align(q: i64) -> &'static str {
    match q {
        1 => "center",
        2 => "right",
        _ => "left",
    }
}

/// The effective default appearance (`/DA`) string for a field: its own or
/// inherited `/DA`, falling back to the AcroForm's default `/DA`. `None` when
/// none is declared anywhere.
fn effective_da(doc: &Document, d: &Dictionary) -> Option<String> {
    if let Some(s) = inherited(doc, d, b"DA").and_then(da_string) {
        return Some(s);
    }
    acroform(doc).and_then(|a| a.get(b"DA").ok()).and_then(da_string)
}

pub(crate) fn da_string(o: &Object) -> Option<String> {
    o.as_str()
        .ok()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// A string value on the field or any ancestor (for inheritable keys like /DA).
pub(crate) fn inherited_str(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    inherited(doc, d, key).and_then(da_string)
}

/// The document's AcroForm dictionary (inline in the catalog or via reference).
pub(crate) fn acroform(doc: &Document) -> Option<&Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let cat = doc.get_dictionary(root).ok()?;
    as_dict(doc, cat.get(b"AcroForm").ok()?).ok()
}

/// True when the form is XFA-backed (the AcroForm carries an /XFA entry).
/// Viewers render the XFA data, so mutating the AcroForm would be misleading.
pub(crate) fn has_xfa(doc: &Document) -> bool {
    acroform(doc).map(|a| a.has(b"XFA")).unwrap_or(false)
}

pub(crate) fn as_dict<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Dictionary, String> {
    match o {
        Object::Reference(id) => doc.get_dictionary(*id).map_err(|e| e.to_string()),
        Object::Dictionary(d) => Ok(d),
        other => Err(format!("expected dict/ref, got {:?}", other)),
    }
}

/// Upper bound on the /Parent chain walk, so a cyclic or malformed PDF
/// (e.g. a field whose /Parent points back to itself) cannot loop forever.
pub(crate) const MAX_PARENT_DEPTH: usize = 128;

pub(crate) fn name_part(d: &Dictionary) -> Option<String> {
    d.get(b"T")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).into_owned())
}

/// Resolve a dictionary's /Parent to a dictionary, if present and well-formed.
pub(crate) fn parent_of<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a Dictionary> {
    as_dict(doc, d.get(b"Parent").ok()?).ok()
}

pub(crate) fn fully_qualified_name(doc: &Document, d: &Dictionary) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = name_part(d) {
        parts.push(p);
    }
    let mut cur = d;
    for _ in 0..MAX_PARENT_DEPTH {
        let Some(parent) = parent_of(doc, cur) else {
            break;
        };
        if let Some(p) = name_part(parent) {
            parts.push(p);
        }
        cur = parent;
    }
    parts.reverse();
    parts.join(".")
}

pub(crate) fn inherited_name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    inherited(doc, d, key).and_then(|o| {
        o.as_name()
            .ok()
            .map(|n| String::from_utf8_lossy(n).into_owned())
    })
}
pub(crate) fn inherited_int(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<i64> {
    inherited(doc, d, key).and_then(|o| o.as_i64().ok())
}
fn inherited<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    if let Ok(o) = d.get(key) {
        return Some(o);
    }
    let mut cur = d;
    for _ in 0..MAX_PARENT_DEPTH {
        let parent = parent_of(doc, cur)?;
        if let Ok(o) = parent.get(key) {
            return Some(o);
        }
        cur = parent;
    }
    None
}

/// Read a field's value-bearing entry (`/V` or `/DV`) as a string. Array
/// values (multi-select choices) are joined with ", "; an empty array or a
/// non-textual value yields `None`.
fn field_value(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key).ok().and_then(|o| match o {
        Object::Array(a) => {
            let parts: Vec<String> = a.iter().filter_map(value_to_string).collect();
            if parts.is_empty() { None } else { Some(parts.join(", ")) }
        }
        other => value_to_string(other),
    })
}

fn value_to_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::String(_, _) => decode_text_string(o).ok(),
        _ => None,
    }
}

pub(crate) fn collect_on_states(doc: &Document, widget: &Dictionary, out: &mut Vec<String>) {
    let Some(ap) = widget.get(b"AP").ok().and_then(|o| as_dict(doc, o).ok()) else {
        return;
    };
    let Some(n) = ap.get(b"N").ok().and_then(|o| as_dict(doc, o).ok()) else {
        return;
    };
    for (k, _) in n.iter() {
        let s = String::from_utf8_lossy(k).into_owned();
        if s != "Off" && !out.contains(&s) {
            out.push(s);
        }
    }
}

pub(crate) fn opt_export(o: &Object) -> String {
    match o {
        Object::Array(a) => a.first().and_then(value_to_string).unwrap_or_default(),
        other => value_to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::read_fields_json;

    #[test]
    fn is_multiline_reads_bit_13() {
        assert!(super::is_multiline(1 << 12));
        assert!(super::is_multiline((1 << 12) | (1 << 1)));
        assert!(!super::is_multiline(0));
        assert!(!super::is_multiline(1 << 11));
        assert!(!super::is_multiline(1 << 13));
    }

    fn fields(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_str(&read_fields_json(bytes).unwrap()).unwrap()
    }

    const VIAJERO: &[u8] = include_bytes!(
        "../../../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf"
    );
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn reads_all_text_fields_of_viajero() {
        let f = fields(VIAJERO);
        assert_eq!(f.as_array().unwrap().len(), 54);
        assert_eq!(f[0]["name"], "viajero.destino");
        assert_eq!(f[0]["type"], "text");
    }

    #[test]
    fn classifies_radio_with_export_states() {
        let f = fields(FICHA);
        let radio = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "beneficiario.tipo_beneficiario")
            .unwrap();
        assert_eq!(radio["type"], "radio");
        let states: Vec<&str> = radio["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert!(states.contains(&"Titular") && states.contains(&"Familiar"));
    }

    #[test]
    fn reports_required_flag_and_widget_layout() {
        let f = fields(VIAJERO);
        let first = &f[0];
        assert!(first["required"].is_boolean());
        // Fields with no NoExport flag report exported = true.
        assert_eq!(first["exported"], true);
        // maxLength is present (null when undeclared, an integer when set).
        assert!(first["maxLength"].is_null() || first["maxLength"].is_u64());
        let widgets = first["widgets"].as_array().unwrap();
        assert!(!widgets.is_empty());
        assert_eq!(widgets[0]["page"], 0);
        assert_eq!(widgets[0]["rect"].as_array().unwrap().len(), 4);
        // Visibility flags are present as booleans on each widget.
        assert!(widgets[0]["hidden"].is_boolean());
        assert!(widgets[0]["print"].is_boolean());
        assert!(widgets[0]["noView"].is_boolean());
    }

    #[test]
    fn widget_visibility_flags_decode_from_annotation_f() {
        use lopdf::{Document, Object, dictionary};
        // A widget with /F = Hidden(2) | Print(4) = 6.
        let field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("hidden_field"),
            "Rect" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0), Object::Real(100.0), Object::Real(20.0),
            ]),
            "F" => Object::Integer(6),
        };
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.set_object(page_id, Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0i64.into(), 0i64.into(), 612i64.into(), 792i64.into()],
        }));
        let field_id = doc.add_object(Object::Dictionary(field));
        // Put the widget on the page so it resolves to a page index.
        if let Ok(p) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            p.set("Annots", Object::Array(vec![Object::Reference(field_id)]));
        }
        doc.set_object(pages_id, Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        }));
        let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field_id)]),
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let f = fields(&bytes);
        let w = &f[0]["widgets"][0];
        assert_eq!(w["hidden"], true);
        assert_eq!(w["print"], true);
        assert_eq!(w["noView"], false);
    }

    #[test]
    fn still_reads_fields_of_xfa_hybrids() {
        const FICHA_XFA: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-xfa.pdf");
        let f = fields(FICHA_XFA);
        assert!(!f.as_array().unwrap().is_empty());
    }

    #[test]
    fn reads_fields_of_xref_stream_pdfs() {
        const OBJSTREAMS: &[u8] =
            include_bytes!("../../../tests/fixtures/generated/ficha-objstreams.pdf");
        let f = fields(OBJSTREAMS);
        let names: Vec<&str> = f
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["name"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"beneficiario.apellidos_nombres"),
            "got: {names:?}"
        );
    }

    #[test]
    fn classifies_dropdown_with_options() {
        let f = fields(FICHA);
        let dd = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "beneficiario.estado_civil")
            .unwrap();
        assert_eq!(dd["type"], "dropdown");
        let opts: Vec<&str> = dd["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert!(opts.contains(&"Soltero"));
    }

    #[test]
    fn array_v_is_joined_as_comma_separated_string() {
        use lopdf::{Document, Object, dictionary};
        // Build a minimal PDF with a listbox field whose /V is an array of strings.
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let field_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let page = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0i64.into(), 0i64.into(), 612i64.into(), 792i64.into()],
        };
        doc.set_object(page_id, Object::Dictionary(page));
        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        };
        doc.set_object(pages_id, Object::Dictionary(pages));
        let field = dictionary! {
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("lang"),
            "V" => Object::Array(vec![
                Object::string_literal("ES"),
                Object::string_literal("PT"),
            ]),
            "Opt" => Object::Array(vec![
                Object::string_literal("ES"),
                Object::string_literal("EN"),
                Object::string_literal("PT"),
            ]),
        };
        doc.set_object(field_id, Object::Dictionary(field));
        let acroform = dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field_id)]),
        };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut pdf_bytes = Vec::new();
        doc.save_to(&mut pdf_bytes).unwrap();

        let f = fields(&pdf_bytes);
        let field_val = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "lang")
            .unwrap();
        assert_eq!(field_val["value"], "ES, PT");
    }

    fn with_multiselect_forms(bytes: &[u8]) -> Vec<u8> {
        use lopdf::{Document, Object};
        let mut doc = Document::load_mem(bytes).unwrap();
        let (id, _) = crate::fill::find_field(&doc, "beneficiario.estado_civil").unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
        // Clear Combo (1<<17) and set Multiselect (1<<21).
        d.set("Ff", Object::Integer((ff & !(1 << 17)) | (1 << 21)));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn multi_select_flag_is_true_for_multiselect_listbox() {
        let base = with_multiselect_forms(FICHA);
        let f = fields(&base);
        let lb = f
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "beneficiario.estado_civil")
            .unwrap();
        assert_eq!(lb["type"], "listbox");
        assert_eq!(lb["multiSelect"], true);
    }

    #[test]
    fn is_password_reads_bit_14() {
        assert!(super::is_password(1 << 13));
        assert!(!super::is_password(0));
        assert!(!super::is_password(1 << 12));
    }

    #[test]
    fn is_comb_reads_bit_25() {
        assert!(super::is_comb(1 << 24));
        assert!(!super::is_comb(0));
        assert!(!super::is_comb(1 << 12));
    }

    #[test]
    fn is_combo_edit_reads_bit_19() {
        assert!(super::is_combo_edit(1 << 18));
        assert!(!super::is_combo_edit(0));
        assert!(!super::is_combo_edit(1 << 17));
    }

    #[test]
    fn quadding_maps_to_alignment_keywords() {
        assert_eq!(super::quadding_to_align(0), "left");
        assert_eq!(super::quadding_to_align(1), "center");
        assert_eq!(super::quadding_to_align(2), "right");
        assert_eq!(super::quadding_to_align(99), "left");
    }

    /// Build a one-page PDF whose AcroForm holds the given field dictionaries.
    fn pdf_with_fields(field_dicts: Vec<lopdf::Dictionary>) -> Vec<u8> {
        use lopdf::{Document, Object, dictionary};
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let page = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0i64.into(), 0i64.into(), 612i64.into(), 792i64.into()],
        };
        doc.set_object(page_id, Object::Dictionary(page));
        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => Object::Integer(1),
        };
        doc.set_object(pages_id, Object::Dictionary(pages));
        let refs: Vec<Object> = field_dicts
            .into_iter()
            .map(|d| Object::Reference(doc.add_object(Object::Dictionary(d))))
            .collect();
        let acroform = dictionary! { "Fields" => Object::Array(refs) };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn reports_multiline_comb_align_and_tooltip_for_text_fields() {
        use lopdf::{Object, dictionary};
        // A multiline, right-aligned text field with a tooltip.
        let area = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("notes"),
            "TU" => Object::string_literal("Additional notes"),
            "Ff" => Object::Integer(1 << 12), // Multiline
            "Q" => Object::Integer(2),         // right
        };
        // A comb text field.
        let comb = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("ssn"),
            "Ff" => Object::Integer(1 << 24), // Comb
            "MaxLen" => Object::Integer(9),
        };
        // A plain single-line text field: every new flag at its default.
        let plain = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("name"),
        };
        let f = fields(&pdf_with_fields(vec![area, comb, plain]));
        let by = |n: &str| {
            f.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == n)
                .unwrap()
                .clone()
        };

        let notes = by("notes");
        assert_eq!(notes["multiline"], true);
        assert_eq!(notes["comb"], false);
        assert_eq!(notes["align"], "right");
        assert_eq!(notes["tooltip"], "Additional notes");

        let ssn = by("ssn");
        assert_eq!(ssn["comb"], true);
        assert_eq!(ssn["multiline"], false);

        let name = by("name");
        assert_eq!(name["multiline"], false);
        assert_eq!(name["comb"], false);
        assert_eq!(name["align"], "left");
        assert_eq!(name["tooltip"], serde_json::Value::Null);
        // editable is a dropdown-only flag; text fields are always false.
        assert_eq!(name["editable"], false);
    }

    #[test]
    fn reports_password_flag_and_default_value() {
        use lopdf::{Object, dictionary};
        // A password field carrying a default value.
        let pin = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("pin"),
            "Ff" => Object::Integer(1 << 13), // Password
            "DV" => Object::string_literal("0000"),
        };
        // A plain field: not a password, no default value.
        let plain = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("name"),
        };
        let f = fields(&pdf_with_fields(vec![pin, plain]));
        let by = |n: &str| {
            f.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == n)
                .unwrap()
                .clone()
        };

        let pin = by("pin");
        assert_eq!(pin["password"], true);
        assert_eq!(pin["defaultValue"], "0000");

        let name = by("name");
        assert_eq!(name["password"], false);
        assert_eq!(name["defaultValue"], serde_json::Value::Null);
    }

    #[test]
    fn reports_editable_flag_for_combo_boxes() {
        use lopdf::{Object, dictionary};
        let editable = dictionary! {
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("country"),
            "Ff" => Object::Integer((1 << 17) | (1 << 18)), // Combo + Edit
            "Opt" => Object::Array(vec![Object::string_literal("AR")]),
        };
        let fixed = dictionary! {
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("city"),
            "Ff" => Object::Integer(1 << 17), // Combo, not editable
            "Opt" => Object::Array(vec![Object::string_literal("BA")]),
        };
        let f = fields(&pdf_with_fields(vec![editable, fixed]));
        let by = |n: &str| {
            f.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == n)
                .unwrap()
                .clone()
        };
        assert_eq!(by("country")["type"], "dropdown");
        assert_eq!(by("country")["editable"], true);
        assert_eq!(by("city")["type"], "dropdown");
        assert_eq!(by("city")["editable"], false);
    }

    /// Emit `tests/fixtures/generated/ficha-multiselect-listbox.pdf` from FICHA
    /// with the Multiselect flag set on `beneficiario.estado_civil`. Idempotent.
    ///
    /// Ignored by default so routine `cargo test` runs don't overwrite the
    /// committed fixture. Run on demand with:
    ///   cargo test emit_ficha_multiselect_listbox_fixture -- --ignored
    #[test]
    #[ignore]
    fn emit_ficha_multiselect_listbox_fixture() {
        use std::path::Path;
        let dest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/generated/ficha-multiselect-listbox.pdf");
        let out = with_multiselect_forms(FICHA);
        std::fs::write(&dest, &out).expect("failed to write fixture");
    }
}
