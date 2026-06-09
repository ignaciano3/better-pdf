use lopdf::{Dictionary, Document, Object};
use serde::Serialize;

#[derive(Serialize)]
pub struct FieldInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub value: Option<String>,
    pub states: Vec<String>,
    pub options: Vec<String>,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
}

/// Parse `data` and return its AcroForm fields as a JSON array string.
pub fn read_fields_json(data: &[u8]) -> Result<String, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let fields = collect_fields(&doc).map_err(|e| e.to_string())?;
    serde_json::to_string(&fields).map_err(|e| e.to_string())
}

fn collect_fields(doc: &Document) -> Result<Vec<FieldInfo>, String> {
    let catalog = as_dict(doc, doc.trailer.get(b"Root").map_err(|e| e.to_string())?)?;
    let acroform = match catalog.get(b"AcroForm") {
        Ok(o) => as_dict(doc, o)?,
        Err(_) => return Ok(Vec::new()),
    };
    let entries = acroform.get(b"Fields").and_then(|o| o.as_array()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in entries {
        let d = as_dict(doc, entry)?;
        out.push(describe_field(doc, d));
    }
    Ok(out)
}

fn describe_field(doc: &Document, d: &Dictionary) -> FieldInfo {
    let name = fully_qualified_name(doc, d);
    let ft = inherited_name(doc, d, b"FT").unwrap_or_default();
    let ff = inherited_int(doc, d, b"Ff").unwrap_or(0);
    let field_type = classify(&ft, ff).to_string();
    let value = d.get(b"V").ok().and_then(value_to_string);

    let mut states = Vec::new();
    collect_on_states(doc, d, &mut states);
    if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kd) = as_dict(doc, k) {
                collect_on_states(doc, kd, &mut states);
            }
        }
    }

    let options = d.get(b"Opt").and_then(|o| o.as_array())
        .map(|a| a.iter().map(opt_export).collect()).unwrap_or_default();

    FieldInfo { name, field_type, value, states, options, read_only: ff & 1 != 0 }
}

pub(crate) fn classify(ft: &str, ff: i64) -> &'static str {
    match ft {
        "Tx" => "text",
        "Btn" => {
            if ff & (1 << 16) != 0 { "pushbutton" }
            else if ff & (1 << 15) != 0 { "radio" }
            else { "checkbox" }
        }
        "Ch" => { if ff & (1 << 17) != 0 { "dropdown" } else { "listbox" } }
        "Sig" => "signature",
        _ => "unknown",
    }
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
    d.get(b"T").ok().and_then(|o| o.as_str().ok()).map(|s| String::from_utf8_lossy(s).into_owned())
}

/// Resolve a dictionary's /Parent to a dictionary, if present and well-formed.
pub(crate) fn parent_of<'a>(doc: &'a Document, d: &'a Dictionary) -> Option<&'a Dictionary> {
    as_dict(doc, d.get(b"Parent").ok()?).ok()
}

pub(crate) fn fully_qualified_name(doc: &Document, d: &Dictionary) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = name_part(d) { parts.push(p); }
    let mut cur = d;
    for _ in 0..MAX_PARENT_DEPTH {
        let Some(parent) = parent_of(doc, cur) else { break };
        if let Some(p) = name_part(parent) { parts.push(p); }
        cur = parent;
    }
    parts.reverse();
    parts.join(".")
}

pub(crate) fn inherited_name(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    inherited(doc, d, key).and_then(|o| o.as_name().ok().map(|n| String::from_utf8_lossy(n).into_owned()))
}
pub(crate) fn inherited_int(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<i64> {
    inherited(doc, d, key).and_then(|o| o.as_i64().ok())
}
fn inherited<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    if let Ok(o) = d.get(key) { return Some(o); }
    let mut cur = d;
    for _ in 0..MAX_PARENT_DEPTH {
        let parent = parent_of(doc, cur)?;
        if let Ok(o) = parent.get(key) { return Some(o); }
        cur = parent;
    }
    None
}

fn value_to_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::String(s, _) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

pub(crate) fn collect_on_states(doc: &Document, widget: &Dictionary, out: &mut Vec<String>) {
    let Some(ap) = widget.get(b"AP").ok().and_then(|o| as_dict(doc, o).ok()) else { return };
    let Some(n) = ap.get(b"N").ok().and_then(|o| as_dict(doc, o).ok()) else { return };
    for (k, _) in n.iter() {
        let s = String::from_utf8_lossy(k).into_owned();
        if s != "Off" && !out.contains(&s) { out.push(s); }
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

    fn fields(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_str(&read_fields_json(bytes).unwrap()).unwrap()
    }

    const VIAJERO: &[u8] =
        include_bytes!("../../../tests/fixtures/Asistencia al Viajero/Formulario asistencia al viajero 1.pdf");
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
        let radio = f.as_array().unwrap().iter()
            .find(|x| x["name"] == "beneficiario.tipo_beneficiario").unwrap();
        assert_eq!(radio["type"], "radio");
        let states: Vec<&str> = radio["states"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(states.contains(&"Titular") && states.contains(&"Familiar"));
    }

    #[test]
    fn classifies_dropdown_with_options() {
        let f = fields(FICHA);
        let dd = f.as_array().unwrap().iter()
            .find(|x| x["name"] == "beneficiario.estado_civil").unwrap();
        assert_eq!(dd["type"], "dropdown");
        let opts: Vec<&str> = dd["options"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(opts.contains(&"Soltero"));
    }
}
