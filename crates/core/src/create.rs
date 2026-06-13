//! Build a new PDF document from scratch (pages + text), reusing the text
//! emission helpers from the draw engine.

use lopdf::{dictionary, Document, Object, Stream};
use serde::Deserialize;

use crate::draw::{emit_text_block, font_dict, standard_14_index, STANDARD_14};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CreateOp {
    AddPage { width: f32, height: f32 },
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

pub fn create_document_json(ops_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<CreateOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid create ops: {e}"))?;

    let pages: Vec<(f32, f32)> = ops
        .iter()
        .filter_map(|o| match o {
            CreateOp::AddPage { width, height } => Some((*width, *height)),
            _ => None,
        })
        .collect();
    if pages.is_empty() {
        return Err("cannot create a document with no pages".to_string());
    }
    for op in &ops {
        if let CreateOp::Text { page, font, .. } = op {
            if *page >= pages.len() {
                return Err(format!("page {page} out of range ({} pages)", pages.len()));
            }
            if standard_14_index(font).is_none() {
                return Err(format!("unknown font: {font}"));
            }
        }
    }

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let mut kids: Vec<Object> = Vec::new();
    for (page_index, (w, h)) in pages.iter().enumerate() {
        let mut content = Vec::new();
        let mut fonts_used: Vec<usize> = Vec::new();
        for op in &ops {
            if let CreateOp::Text {
                page,
                x,
                y,
                size,
                font,
                color,
                text,
                line_height,
            } = op
            {
                if *page != page_index {
                    continue;
                }
                let idx = standard_14_index(font).unwrap();
                if !fonts_used.contains(&idx) {
                    fonts_used.push(idx);
                }
                emit_text_block(
                    &mut content,
                    &format!("BPF{idx}"),
                    *x,
                    *y,
                    *size,
                    *color,
                    text,
                    *line_height,
                );
            }
        }

        let mut font_res = lopdf::Dictionary::new();
        for idx in &fonts_used {
            let fid = doc.add_object(Object::Dictionary(font_dict(STANDARD_14[*idx])));
            font_res.set(format!("BPF{idx}"), Object::Reference(fid));
        }
        let resources = dictionary! { "Font" => Object::Dictionary(font_res) };

        let content_id = doc.add_object(Object::Stream(Stream::new(
            lopdf::Dictionary::new(),
            content,
        )));
        let page_dict = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(*w),
                Object::Real(*h),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        kids.push(Object::Reference(page_id));
    }

    let count = kids.len() as i64;
    let pages_dict = dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count),
    };
    doc.set_object(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    #[test]
    fn creates_single_page_doc() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"Pages"));
    }

    #[test]
    fn page_has_mediabox() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let mb = page.get(b"MediaBox").unwrap().as_array().unwrap();
        assert_eq!(mb.len(), 4);
        assert!((mb[2].as_float().unwrap() - 595.0).abs() < 0.5);
        assert!((mb[3].as_float().unwrap() - 842.0).abs() < 0.5);
    }

    #[test]
    fn text_drawn_on_created_page() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.iter().any(|(k, _)| k.starts_with(b"BPF")));
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains("(Hello) Tj"), "content: {s}");
    }

    #[test]
    fn multiple_pages_in_order() {
        let out = create_document_json(r#"[{"op":"addPage","width":100,"height":200},{"op":"addPage","width":300,"height":400}]"#).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(pages.len(), 2);
        let p0 = doc.get_dictionary(pages[0].1).unwrap();
        let mb0 = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb0[2].as_float().unwrap() - 100.0).abs() < 0.5);
    }

    #[test]
    fn errors_on_no_pages() {
        let r = create_document_json(r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#);
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#);
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn output_parses_and_is_nonempty() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#).unwrap();
        assert!(out.starts_with(b"%PDF-"));
        assert!(out.len() > 100);
    }
}
