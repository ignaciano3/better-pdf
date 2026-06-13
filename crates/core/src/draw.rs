//! Draw engine: apply draw ops (text, etc.) to existing PDF pages via
//! incremental update.

use lopdf::{dictionary, Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use serde::Deserialize;

use crate::appearance::{encode_winansi, escape_pdf_literal};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum DrawOp {
    Text {
        page: usize,
        x: f32,
        y: f32,
        size: f32,
        font: String,
        color: [f32; 3],
        text: String,
        #[serde(rename = "lineHeight")]
        line_height: Option<f32>,
    },
}

const STANDARD_14: &[&str] = &[
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
];

/// Format a float with up to 2 decimal places, trimming trailing zeros.
fn fmt_num(v: f32) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    if (rounded - rounded.floor()).abs() < 0.001 {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{:.2}", rounded);
        let s = s.trim_end_matches('0');
        s.to_string()
    }
}

/// Apply draw ops from a JSON string to `data` and return new PDF bytes
/// (incremental save).
pub fn apply_draw_ops_json(data: &[u8], ops_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<DrawOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid draw ops: {e}"))?;

    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let page_count = doc.get_pages().len();

    // Validate ALL ops before mutating anything
    for op in &ops {
        match op {
            DrawOp::Text { page, font, .. } => {
                if *page >= page_count {
                    return Err(format!(
                        "page {page} out of range ({page_count} pages)"
                    ));
                }
                if !STANDARD_14.contains(&font.as_str()) {
                    return Err(format!("unknown font: {font}"));
                }
            }
        }
    }

    // Group ops by page index (preserving op order within each page)
    let mut page_ops: Vec<(usize, Vec<&DrawOp>)> = Vec::new();
    for op in &ops {
        let page_idx = match op {
            DrawOp::Text { page, .. } => *page,
        };
        if let Some(entry) = page_ops.iter_mut().find(|(idx, _)| *idx == page_idx) {
            entry.1.push(op);
        } else {
            page_ops.push((page_idx, vec![op]));
        }
    }

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Create q and Q streams once, shared across all touched pages
    let q_id = inc
        .new_document
        .add_object(Object::Stream(Stream::new(Dictionary::new(), b"q\n".to_vec())));
    let q_ref_id = inc
        .new_document
        .add_object(Object::Stream(Stream::new(Dictionary::new(), b"Q\n".to_vec())));

    // Pre-create font objects (one per unique font used, keyed by STANDARD_14 index)
    let mut font_cache: std::collections::HashMap<usize, ObjectId> =
        std::collections::HashMap::new();

    // Process each touched page
    for (page_idx, page_op_list) in &page_ops {
        // Build one stream containing a separate BT...ET block per op.
        // Each BT resets the text matrix to identity, making each op's Td
        // absolute rather than relative to the previous line origin.
        let mut stream_content = Vec::new();

        for op in page_op_list {
            match op {
                DrawOp::Text {
                    x,
                    y,
                    size,
                    font,
                    color,
                    text,
                    line_height,
                    page: _,
                } => {
                    let font_idx = STANDARD_14.iter().position(|&f| f == font.as_str()).unwrap();
                    let leading = line_height.unwrap_or(size * 1.15);
                    let [r, g, b] = color;

                    // Ensure font object exists
                    if !font_cache.contains_key(&font_idx) {
                        let font_dict = dictionary! {
                            "Type" => Object::Name(b"Font".to_vec()),
                            "Subtype" => Object::Name(b"Type1".to_vec()),
                            "BaseFont" => Object::Name(font.as_bytes().to_vec()),
                            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
                        };
                        let fid = inc
                            .new_document
                            .add_object(Object::Dictionary(font_dict));
                        font_cache.insert(font_idx, fid);
                    }

                    let lines: Vec<&str> = text.split('\n').collect();

                    // One self-contained BT...ET block per op; BT resets the
                    // text matrix to identity so Td gives absolute positioning.
                    stream_content.extend_from_slice(b"BT\n");
                    stream_content.extend_from_slice(
                        format!("/BPF{font_idx} {} Tf\n", fmt_num(*size)).as_bytes(),
                    );
                    stream_content.extend_from_slice(
                        format!(
                            "{} {} {} rg\n",
                            fmt_num(*r),
                            fmt_num(*g),
                            fmt_num(*b)
                        )
                        .as_bytes(),
                    );
                    stream_content
                        .extend_from_slice(format!("{} TL\n", fmt_num(leading)).as_bytes());
                    stream_content.extend_from_slice(
                        format!("{} {} Td\n", fmt_num(*x), fmt_num(*y)).as_bytes(),
                    );

                    for (i, line) in lines.iter().enumerate() {
                        let encoded = encode_winansi(line);
                        let escaped = escape_pdf_literal(&encoded);
                        let escaped_str = String::from_utf8_lossy(&escaped).into_owned();
                        if i == 0 {
                            stream_content
                                .extend_from_slice(format!("({escaped_str}) Tj\n").as_bytes());
                        } else {
                            stream_content
                                .extend_from_slice(format!("T*\n({escaped_str}) Tj\n").as_bytes());
                        }
                    }
                    stream_content.extend_from_slice(b"ET\n");
                }
            }
        }

        let draw_id = inc.new_document.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            stream_content,
        )));
        let draw_ids = vec![draw_id];

        // Get the page ObjectId from the previous document (page_idx is 0-based)
        let page_id = {
            let prev = inc.get_prev_documents();
            let mut sorted_pages: Vec<(u32, ObjectId)> = prev.get_pages().into_iter().collect();
            sorted_pages.sort_by_key(|(num, _)| *num);
            sorted_pages[*page_idx].1
        };

        // Clone the page into the new document so we can mutate it
        inc.opt_clone_object_to_new_document(page_id)
            .map_err(|e| e.to_string())?;

        // Build new Contents array: [q_ref, ...original, Q_ref, draw_ref...]
        {
            // Read and clone the existing Contents value first so the borrow ends
            // before we mutate inc.new_document (needed for Issue 2 below).
            let existing_contents: Option<Object> = dict_mut(&mut inc, page_id)?
                .get(b"Contents")
                .ok()
                .cloned();

            // Issue 3: missing /Contents is valid (blank page); treat as empty.
            // Issue 2: a direct Stream in /Contents must be made indirect —
            //          streams must be indirect objects when referenced from an
            //          array. Promote it by adding it to new_document.
            let mut arr: Vec<Object> = match existing_contents {
                Some(Object::Array(a)) => a,
                Some(Object::Stream(s)) => {
                    // Direct stream — make it indirect so the array only holds refs.
                    let indirect_id = inc
                        .new_document
                        .add_object(Object::Stream(s));
                    vec![Object::Reference(indirect_id)]
                }
                Some(single) => vec![single],
                None => Vec::new(), // missing /Contents — blank page
            };
            // Wrap: q, ...original, Q, draw...
            arr.insert(0, Object::Reference(q_id));
            arr.push(Object::Reference(q_ref_id));
            for draw_id in &draw_ids {
                arr.push(Object::Reference(*draw_id));
            }
            dict_mut(&mut inc, page_id)?.set("Contents", Object::Array(arr));
        }

        // Collect unique fonts used on this page
        let mut fonts_on_page: Vec<(usize, String)> = Vec::new();
        for op in page_op_list {
            let DrawOp::Text { font, .. } = op;
            let idx = STANDARD_14.iter().position(|&f| f == font.as_str()).unwrap();
            if !fonts_on_page.iter().any(|(i, _)| *i == idx) {
                fonts_on_page.push((idx, font.clone()));
            }
        }

        for (font_idx, _font_name) in &fonts_on_page {
            let font_obj_id = *font_cache.get(font_idx).unwrap();
            let key = format!("BPF{font_idx}");
            register_font(&mut inc, page_id, &key, font_obj_id)?;
        }
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Register `key -> font_id` under the page's /Resources/Font.
fn register_font(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    key: &str,
    font_id: ObjectId,
) -> Result<(), String> {
    // Page is already cloned; check if Resources is a reference
    let res_ref = match dict_mut(inc, page_id)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };

    match res_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            set_font(dict_mut(inc, id)?, key, font_id);
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
            set_font(res, key, font_id);
        }
    }
    Ok(())
}

fn set_font(res: &mut Dictionary, key: &str, font_id: ObjectId) {
    if !res.has(b"Font") {
        res.set("Font", Object::Dictionary(Dictionary::new()));
    }
    if let Ok(font_dict) = res.get_mut(b"Font").and_then(Object::as_dict_mut) {
        font_dict.set(key.as_bytes().to_vec(), Object::Reference(font_id));
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
    use super::*;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn ops(json: &str) -> Vec<u8> {
        apply_draw_ops_json(FICHA, json).unwrap()
    }

    fn last_draw_stream_content(out: &[u8]) -> String {
        let doc = Document::load_mem(out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let arr = match dict.get(b"Contents").unwrap() {
            lopdf::Object::Array(a) => a.clone(),
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_array().unwrap().clone(),
            _ => panic!("expected contents array"),
        };
        let draw_id = arr.last().unwrap().as_reference().unwrap();
        let stream = doc.get_object(draw_id).unwrap().as_stream().unwrap();
        let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        String::from_utf8_lossy(&content).into_owned()
    }

    #[test]
    fn output_is_incremental() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#);
        assert_eq!(&out[..FICHA.len()], FICHA);
        assert!(out.len() > FICHA.len());
    }

    #[test]
    fn page_contents_grow_and_balance() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#);
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let arr = match dict.get(b"Contents").unwrap() {
            lopdf::Object::Array(a) => a.clone(),
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_array().unwrap().clone(),
            _ => panic!("expected contents array"),
        };
        assert!(arr.len() >= 3);
        let s = last_draw_stream_content(&out);
        assert!(s.contains("(Hello) Tj"), "content was: {s}");
        assert!(s.contains("BT") && s.contains("ET"));
    }

    #[test]
    fn font_registered_in_page_resources() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Times-Bold","color":[0,0,0],"text":"x"}]"#);
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!(),
        };
        let fonts = match res.get(b"Font").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!(),
        };
        assert!(fonts.iter().any(|(k, _)| k.starts_with(b"BPF")));
    }

    #[test]
    fn errors_on_bad_page() {
        let r = apply_draw_ops_json(FICHA, r#"[{"op":"text","page":999,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = apply_draw_ops_json(FICHA, r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn multiline_emits_multiple_tj() {
        let out = ops(r#"[{"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"a\nb"}]"#);
        let s = last_draw_stream_content(&out);
        assert!(s.matches(" Tj").count() == 2, "content was: {s}");
    }

    #[test]
    fn ops_on_same_page_are_absolutely_positioned() {
        let out = ops(r#"[
            {"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"first"},
            {"op":"text","page":0,"x":200,"y":300,"size":12,"font":"Helvetica","color":[0,0,0],"text":"second"}
        ]"#);
        let s = last_draw_stream_content(&out);
        assert_eq!(s.matches("BT").count(), 2, "one BT/ET block per op, content: {s}");
        assert!(s.contains("50 700 Td"));
        assert!(s.contains("200 300 Td"), "second op must be absolutely positioned, content: {s}");
    }
}
