//! Build a new PDF document from scratch (pages + text + images), reusing the
//! text and image emission helpers from the draw engine.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use serde::Deserialize;

use crate::draw::{emit_image_op, emit_text_block, font_dict, standard_14_index, STANDARD_14};

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
    Image {
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(rename = "imageOffset")]
        image_offset: usize,
        #[serde(rename = "imageLength")]
        image_length: usize,
    },
}

pub fn create_document_json(ops_json: &str, images: &[u8]) -> Result<Vec<u8>, String> {
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
    // Validation pass: check all ops before building anything
    for op in &ops {
        match op {
            CreateOp::Text { page, font, .. } => {
                if *page >= pages.len() {
                    return Err(format!("page {page} out of range ({} pages)", pages.len()));
                }
                if standard_14_index(font).is_none() {
                    return Err(format!("unknown font: {font}"));
                }
            }
            CreateOp::Image {
                page,
                image_offset,
                image_length,
                ..
            } => {
                if *page >= pages.len() {
                    return Err(format!("page {page} out of range ({} pages)", pages.len()));
                }
                let end = image_offset
                    .checked_add(*image_length)
                    .ok_or_else(|| "image range out of bounds".to_string())?;
                if end > images.len() {
                    return Err("image range out of bounds".to_string());
                }
                crate::appearance::signature_image(&images[*image_offset..end])?;
            }
            CreateOp::AddPage { .. } => {}
        }
    }

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // Global image counter for unique XObject keys
    let mut img_counter: usize = 0;

    let mut kids: Vec<Object> = Vec::new();
    for (page_index, (w, h)) in pages.iter().enumerate() {
        let mut content = Vec::new();
        let mut font_res = Dictionary::new();
        let mut xobject_res = Dictionary::new();

        // Single ordered pass over ops for this page to preserve z-order
        for op in &ops {
            match op {
                CreateOp::Text {
                    page,
                    x,
                    y,
                    size,
                    font,
                    color,
                    text,
                    line_height,
                } if *page == page_index => {
                    let idx = standard_14_index(font).unwrap();
                    // Register font resource if not already added
                    let key = format!("BPF{idx}");
                    if !font_res.has(key.as_bytes()) {
                        let fid = doc.add_object(Object::Dictionary(font_dict(STANDARD_14[idx])));
                        font_res.set(key.clone(), Object::Reference(fid));
                    }
                    emit_text_block(
                        &mut content,
                        &key,
                        *x,
                        *y,
                        *size,
                        *color,
                        text,
                        *line_height,
                    );
                }
                CreateOp::Image {
                    page,
                    x,
                    y,
                    width,
                    height,
                    image_offset,
                    image_length,
                } if *page == page_index => {
                    let end = image_offset + image_length;
                    let img = crate::appearance::signature_image(&images[*image_offset..end])?;
                    let stream = crate::appearance::build_signature_image_xobject(img);
                    let xid = doc.add_object(Object::Stream(stream));
                    let key = format!("BPI{img_counter}");
                    img_counter += 1;
                    xobject_res.set(key.clone(), Object::Reference(xid));
                    emit_image_op(&mut content, &key, *x, *y, *width, *height);
                }
                _ => {}
            }
        }

        // Build resources dict, only including sub-dicts that have entries
        let mut resources = Dictionary::new();
        if font_res.len() > 0 {
            resources.set("Font", Object::Dictionary(font_res));
        }
        if xobject_res.len() > 0 {
            resources.set("XObject", Object::Dictionary(xobject_res));
        }

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

    fn tiny_png() -> &'static [u8] {
        &[
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
            0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
            0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn creates_single_page_doc() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"Pages"));
    }

    #[test]
    fn page_has_mediabox() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[]).unwrap();
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
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#, &[]).unwrap();
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
        let out = create_document_json(r#"[{"op":"addPage","width":100,"height":200},{"op":"addPage","width":300,"height":400}]"#, &[]).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(pages.len(), 2);
        let p0 = doc.get_dictionary(pages[0].1).unwrap();
        let mb0 = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb0[2].as_float().unwrap() - 100.0).abs() < 0.5);
    }

    #[test]
    fn errors_on_no_pages() {
        let r = create_document_json(r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#, &[]);
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_text_page_out_of_range() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":1,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#, &[]);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#, &[]);
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn output_parses_and_is_nonempty() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[]).unwrap();
        assert!(out.starts_with(b"%PDF-"));
        assert!(out.len() > 100);
    }

    #[test]
    fn creates_doc_with_image() {
        let png = tiny_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = create_document_json(&json, png).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjs = res.get(b"XObject").unwrap().as_dict().unwrap();
        let bpi_entry = xobjs.iter().find(|(k, _)| k.starts_with(b"BPI"));
        assert!(bpi_entry.is_some(), "expected a BPI* key in XObject resources");
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains("/BPI0 Do"), "content stream should contain '/BPI0 Do', got: {s}");
    }

    #[test]
    fn image_page_out_of_range_errors() {
        let png = tiny_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":1,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let r = create_document_json(&json, png);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn image_range_out_of_bounds_errors() {
        let png = tiny_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{}}}]"#,
            len + 1
        );
        let r = create_document_json(&json, png);
        assert!(r.unwrap_err().contains("out of bounds"));
    }

    #[test]
    fn image_info_via_signature_image() {
        let img = crate::appearance::signature_image(tiny_png()).unwrap();
        let info = img.info();
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
    }
}
