//! Build a new PDF document from scratch (pages + text + images), reusing the
//! text and image emission helpers from the draw engine.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use serde::Deserialize;
use std::collections::HashSet;

use crate::draw::{
    emit_ellipse, emit_image_op, emit_line, emit_rectangle, emit_text_block, extgstate_dict,
    font_dict, standard_14_index, STANDARD_14,
};

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
    Line {
        page: usize,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: Option<f32>,
        color: Option<[f32; 3]>,
        opacity: Option<f32>,
    },
    Rectangle {
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Option<[f32; 3]>,
        #[serde(rename = "borderColor")]
        border_color: Option<[f32; 3]>,
        #[serde(rename = "borderWidth")]
        border_width: Option<f32>,
        opacity: Option<f32>,
    },
    Ellipse {
        page: usize,
        x: f32,
        y: f32,
        #[serde(rename = "xScale")]
        x_scale: f32,
        #[serde(rename = "yScale")]
        y_scale: f32,
        color: Option<[f32; 3]>,
        #[serde(rename = "borderColor")]
        border_color: Option<[f32; 3]>,
        #[serde(rename = "borderWidth")]
        border_width: Option<f32>,
        opacity: Option<f32>,
    },
}

#[derive(Deserialize)]
struct Border {
    color: [f32; 3],
    width: f32,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum FieldDef {
    Text {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        value: Option<String>,
        #[serde(rename = "maxLength")]
        max_length: Option<i64>,
        multiline: Option<bool>,
        #[serde(default)]
        required: bool,
        #[serde(rename = "readOnly", default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
    },
    CheckBox {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        size: f32,
        #[serde(default)]
        checked: bool,
        #[serde(rename = "onValue")]
        on_value: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(rename = "readOnly", default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
    },
    RadioGroup {
        name: String,
        selected: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(rename = "readOnly", default)]
        read_only: bool,
        tooltip: Option<String>,
        options: Vec<RadioOption>,
    },
    #[serde(rename = "choice")]
    Choice {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        combo: bool,
        options: Vec<String>,
        selected: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(rename = "readOnly", default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
    },
    #[serde(rename = "signature")]
    Signature {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        required: bool,
        #[serde(rename = "readOnly", default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
    },
}

#[derive(Deserialize)]
struct RadioOption {
    value: String,
    page: usize,
    x: f32,
    y: f32,
    size: f32,
}

/// Build a minimal Form XObject with no font resources.
fn button_xobject(size: f32, content: Vec<u8>) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("FormType", Object::Integer(1));
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(size),
            Object::Real(size),
        ]),
    );
    Stream::new(dict, content).with_compression(false)
}

/// Off appearance: empty stream (blank).
fn button_off_appearance(size: f32) -> Stream {
    button_xobject(size, Vec::new())
}

/// On appearance for checkboxes: a tick mark via two line segments.
fn checkbox_on_appearance(size: f32) -> Stream {
    use crate::draw::fmt_num;
    let p = size * 0.2;
    let t = size * 0.12;
    let content = format!(
        "q {} w 0 0 0 RG {} {} m {} {} l {} {} l S Q",
        fmt_num(t),
        fmt_num(p),
        fmt_num(size * 0.5),
        fmt_num(size * 0.42),
        fmt_num(p),
        fmt_num(size - p),
        fmt_num(size - p),
    )
    .into_bytes();
    button_xobject(size, content)
}

/// On appearance for radio buttons: filled black circle.
fn radio_on_appearance(size: f32) -> Stream {
    use crate::draw::fmt_num;
    let c = size / 2.0;
    let r = size * 0.3;
    let k = 0.5523 * r;
    let content = format!(
        "q 0 0 0 rg {} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c f Q",
        fmt_num(c + r), fmt_num(c),
        fmt_num(c + r), fmt_num(c + k), fmt_num(c + k), fmt_num(c + r), fmt_num(c), fmt_num(c + r),
        fmt_num(c - k), fmt_num(c + r), fmt_num(c - r), fmt_num(c + k), fmt_num(c - r), fmt_num(c),
        fmt_num(c - r), fmt_num(c - k), fmt_num(c - k), fmt_num(c - r), fmt_num(c), fmt_num(c - r),
        fmt_num(c + k), fmt_num(c - r), fmt_num(c + r), fmt_num(c - k), fmt_num(c + r), fmt_num(c),
    )
    .into_bytes();
    button_xobject(size, content)
}

pub fn create_document_json(ops_json: &str, images: &[u8], fields_json: &str) -> Result<Vec<u8>, String> {
    let ops: Vec<CreateOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid create ops: {e}"))?;

    // Parse fields, treating "" as empty array
    let effective_fields_json = if fields_json.is_empty() { "[]" } else { fields_json };
    let fields: Vec<FieldDef> =
        serde_json::from_str(effective_fields_json).map_err(|e| format!("invalid fields: {e}"))?;

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
            CreateOp::Line {
                page,
                x1,
                y1,
                x2,
                y2,
                thickness,
                color,
                opacity,
            } => {
                if *page >= pages.len() {
                    return Err(format!("page {page} out of range ({} pages)", pages.len()));
                }
                if let Some(o) = opacity {
                    if !o.is_finite() || *o < 0.0 || *o > 1.0 {
                        return Err("opacity must be in 0..1".to_string());
                    }
                }
                if let Some(t) = thickness {
                    if !t.is_finite() || *t < 0.0 {
                        return Err("thickness must be >= 0".to_string());
                    }
                }
                for &v in &[*x1, *y1, *x2, *y2] {
                    if !v.is_finite() {
                        return Err("invalid coordinate".to_string());
                    }
                }
                if let Some(c) = color {
                    for &v in c.iter() {
                        if !v.is_finite() {
                            return Err("invalid color".to_string());
                        }
                    }
                }
            }
            CreateOp::Rectangle {
                page,
                x,
                y,
                width,
                height,
                color,
                border_color,
                border_width,
                opacity,
            } => {
                if *page >= pages.len() {
                    return Err(format!("page {page} out of range ({} pages)", pages.len()));
                }
                if let Some(o) = opacity {
                    if !o.is_finite() || *o < 0.0 || *o > 1.0 {
                        return Err("opacity must be in 0..1".to_string());
                    }
                }
                if let Some(bw) = border_width {
                    if !bw.is_finite() || *bw < 0.0 {
                        return Err("borderWidth must be >= 0".to_string());
                    }
                }
                for &v in &[*x, *y, *width, *height] {
                    if !v.is_finite() {
                        return Err("invalid coordinate".to_string());
                    }
                }
                if *width <= 0.0 {
                    return Err("width must be > 0".to_string());
                }
                if *height <= 0.0 {
                    return Err("height must be > 0".to_string());
                }
                if let Some(c) = color {
                    for &v in c.iter() {
                        if !v.is_finite() {
                            return Err("invalid color".to_string());
                        }
                    }
                }
                if let Some(c) = border_color {
                    for &v in c.iter() {
                        if !v.is_finite() {
                            return Err("invalid color".to_string());
                        }
                    }
                }
            }
            CreateOp::Ellipse {
                page,
                x,
                y,
                x_scale,
                y_scale,
                color,
                border_color,
                border_width,
                opacity,
            } => {
                if *page >= pages.len() {
                    return Err(format!("page {page} out of range ({} pages)", pages.len()));
                }
                if let Some(o) = opacity {
                    if !o.is_finite() || *o < 0.0 || *o > 1.0 {
                        return Err("opacity must be in 0..1".to_string());
                    }
                }
                if let Some(bw) = border_width {
                    if !bw.is_finite() || *bw < 0.0 {
                        return Err("borderWidth must be >= 0".to_string());
                    }
                }
                for &v in &[*x, *y, *x_scale, *y_scale] {
                    if !v.is_finite() {
                        return Err("invalid coordinate".to_string());
                    }
                }
                if *x_scale <= 0.0 {
                    return Err("xScale must be > 0".to_string());
                }
                if *y_scale <= 0.0 {
                    return Err("yScale must be > 0".to_string());
                }
                if let Some(c) = color {
                    for &v in c.iter() {
                        if !v.is_finite() {
                            return Err("invalid color".to_string());
                        }
                    }
                }
                if let Some(c) = border_color {
                    for &v in c.iter() {
                        if !v.is_finite() {
                            return Err("invalid color".to_string());
                        }
                    }
                }
            }
            CreateOp::AddPage { .. } => {}
        }
    }

    // Validate fields
    {
        let mut seen_names: HashSet<&str> = HashSet::new();
        for field in &fields {
            match field {
                FieldDef::Text { name, page, x, y, width, height, max_length, .. } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= pages.len() {
                        return Err(format!("field page {page} out of range ({} pages)", pages.len()));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("field x/y must be finite".to_string());
                    }
                    if !width.is_finite() || *width <= 0.0 {
                        return Err("field width must be finite and > 0".to_string());
                    }
                    if !height.is_finite() || *height <= 0.0 {
                        return Err("field height must be finite and > 0".to_string());
                    }
                    if let Some(ml) = max_length {
                        if *ml < 0 {
                            return Err("field maxLength must be >= 0".to_string());
                        }
                    }
                }
                FieldDef::CheckBox { name, page, x, y, size, on_value, .. } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= pages.len() {
                        return Err(format!("field page {page} out of range ({} pages)", pages.len()));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("field x/y must be finite".to_string());
                    }
                    if !size.is_finite() || *size <= 0.0 {
                        return Err("checkbox size must be finite and > 0".to_string());
                    }
                    if let Some(ov) = on_value {
                        if ov == "Off" {
                            return Err("checkbox onValue must not be \"Off\"".to_string());
                        }
                    }
                }
                FieldDef::RadioGroup { name, selected, options, .. } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if options.is_empty() {
                        return Err(format!("radioGroup \"{name}\" must have at least one option"));
                    }
                    let mut seen_values: HashSet<&str> = HashSet::new();
                    for opt in options {
                        if opt.value.is_empty() {
                            return Err("radio option value must not be empty".to_string());
                        }
                        if opt.value == "Off" {
                            return Err("radio option value must not be \"Off\"".to_string());
                        }
                        if !seen_values.insert(opt.value.as_str()) {
                            return Err(format!("duplicate radio option value: {}", opt.value));
                        }
                        if opt.page >= pages.len() {
                            return Err(format!("radio option page {} out of range ({} pages)", opt.page, pages.len()));
                        }
                        if !opt.x.is_finite() || !opt.y.is_finite() {
                            return Err("radio option x/y must be finite".to_string());
                        }
                        if !opt.size.is_finite() || opt.size <= 0.0 {
                            return Err("radio option size must be finite and > 0".to_string());
                        }
                    }
                    if let Some(sel) = selected {
                        if !options.iter().any(|o| &o.value == sel) {
                            return Err(format!("radioGroup \"{name}\" selected value \"{sel}\" is not in options"));
                        }
                    }
                }
                FieldDef::Choice { name, page, x, y, width, height, options, selected, .. } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= pages.len() {
                        return Err(format!("field page {page} out of range ({} pages)", pages.len()));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("field x/y must be finite".to_string());
                    }
                    if !width.is_finite() || *width <= 0.0 {
                        return Err("field width must be finite and > 0".to_string());
                    }
                    if !height.is_finite() || *height <= 0.0 {
                        return Err("field height must be finite and > 0".to_string());
                    }
                    if options.is_empty() {
                        return Err(format!("choice field \"{name}\" must have at least one option"));
                    }
                    let mut seen_opts: HashSet<&str> = HashSet::new();
                    for opt in options {
                        if !seen_opts.insert(opt.as_str()) {
                            return Err(format!("duplicate choice option: {opt}"));
                        }
                    }
                    if let Some(sel) = selected {
                        if !options.iter().any(|o| o == sel) {
                            return Err(format!("choice field \"{name}\" selected value \"{sel}\" is not in options"));
                        }
                    }
                }
                FieldDef::Signature { name, page, x, y, width, height, .. } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= pages.len() {
                        return Err(format!("field page {page} out of range ({} pages)", pages.len()));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("field x/y must be finite".to_string());
                    }
                    if !width.is_finite() || *width <= 0.0 {
                        return Err("field width must be finite and > 0".to_string());
                    }
                    if !height.is_finite() || *height <= 0.0 {
                        return Err("field height must be finite and > 0".to_string());
                    }
                }
            }
        }
    }

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // Global image counter for unique XObject keys
    let mut img_counter: usize = 0;

    // Global ExtGState counter for unique BPG keys across all pages
    let mut gs_counter: usize = 0;

    let mut kids: Vec<Object> = Vec::new();
    let mut page_ids: Vec<lopdf::ObjectId> = Vec::new();
    for (page_index, (w, h)) in pages.iter().enumerate() {
        let mut content = Vec::new();
        let mut font_res = Dictionary::new();
        let mut xobject_res = Dictionary::new();
        let mut extgstate_res = Dictionary::new();

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
                CreateOp::Line {
                    page,
                    x1,
                    y1,
                    x2,
                    y2,
                    thickness,
                    color,
                    opacity,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id =
                            doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    emit_line(
                        &mut content,
                        gs_key.as_deref(),
                        *x1,
                        *y1,
                        *x2,
                        *y2,
                        thickness.unwrap_or(1.0),
                        color.unwrap_or([0.0, 0.0, 0.0]),
                    );
                }
                CreateOp::Rectangle {
                    page,
                    x,
                    y,
                    width,
                    height,
                    color,
                    border_color,
                    border_width,
                    opacity,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id =
                            doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    emit_rectangle(
                        &mut content,
                        gs_key.as_deref(),
                        *x,
                        *y,
                        *width,
                        *height,
                        *color,
                        *border_color,
                        *border_width,
                    );
                }
                CreateOp::Ellipse {
                    page,
                    x,
                    y,
                    x_scale,
                    y_scale,
                    color,
                    border_color,
                    border_width,
                    opacity,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id =
                            doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    emit_ellipse(
                        &mut content,
                        gs_key.as_deref(),
                        *x,
                        *y,
                        *x_scale,
                        *y_scale,
                        *color,
                        *border_color,
                        *border_width,
                    );
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
        if extgstate_res.len() > 0 {
            resources.set("ExtGState", Object::Dictionary(extgstate_res));
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
        page_ids.push(page_id);
    }

    let count = kids.len() as i64;
    let pages_dict = dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count),
    };
    doc.set_object(pages_id, Object::Dictionary(pages_dict));

    // Build AcroForm and field widgets if any fields are defined
    let acro_form_ref = if !fields.is_empty() {
        // Shared Helv font for all field appearances
        let helv = doc.add_object(Object::Dictionary(font_dict("Helvetica")));
        let widths = crate::appearance::helvetica_widths();

        let mut acro_fields: Vec<Object> = Vec::new();
        // Track which field object ids go on which page: page_index -> Vec<ObjectId>
        let mut page_annots: Vec<Vec<lopdf::ObjectId>> = vec![Vec::new(); page_ids.len()];

        for field in &fields {
            match field {
                FieldDef::Text {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    value,
                    max_length,
                    multiline,
                    required,
                    read_only,
                    tooltip,
                    border,
                    background,
                } => {
                    let val_str = value.clone().unwrap_or_default();
                    let val_bytes = crate::appearance::encode_winansi(&val_str);

                    let content = crate::appearance::text_appearance_content(
                        &val_bytes,
                        12.0,
                        *width,
                        *height,
                        0,
                        "0 g",
                        "Helv",
                        &widths,
                    );
                    let ap_stream = crate::appearance::build_appearance_xobject(
                        content, *width, *height, "Helv", helv,
                    );
                    let ap_id = doc.add_object(Object::Stream(ap_stream));

                    let flags: i64 = ((*read_only as i64) << 0)
                        | ((*required as i64) << 1)
                        | ((multiline.unwrap_or(false) as i64) << 12);

                    let rect = Object::Array(vec![
                        Object::Real(*x),
                        Object::Real(*y),
                        Object::Real(*x + *width),
                        Object::Real(*y + *height),
                    ]);

                    let mut field_dict = Dictionary::new();
                    field_dict.set("Type", Object::Name(b"Annot".to_vec()));
                    field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
                    field_dict.set("FT", Object::Name(b"Tx".to_vec()));
                    field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
                    field_dict.set("Rect", rect);
                    field_dict.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
                    field_dict.set("V", Object::string_literal(val_bytes));
                    field_dict.set("Ff", Object::Integer(flags));
                    field_dict.set(
                        "AP",
                        Object::Dictionary(dictionary! {
                            "N" => Object::Reference(ap_id)
                        }),
                    );
                    field_dict.set("P", Object::Reference(page_ids[*page]));

                    if let Some(ml) = max_length {
                        field_dict.set("MaxLen", Object::Integer(*ml));
                    }
                    if let Some(tip) = tooltip {
                        if !tip.is_empty() {
                            field_dict.set("TU", Object::string_literal(tip.as_bytes().to_vec()));
                        }
                    }

                    // MK dict: BG (background), BC (border color), BS (border style)
                    let mut mk = Dictionary::new();
                    if let Some(bg) = background {
                        mk.set(
                            "BG",
                            Object::Array(vec![
                                Object::Real(bg[0]),
                                Object::Real(bg[1]),
                                Object::Real(bg[2]),
                            ]),
                        );
                    }
                    if let Some(b) = border {
                        mk.set(
                            "BC",
                            Object::Array(vec![
                                Object::Real(b.color[0]),
                                Object::Real(b.color[1]),
                                Object::Real(b.color[2]),
                            ]),
                        );
                        // Add border style if width != 1
                        if (b.width - 1.0).abs() > 0.001 {
                            field_dict.set(
                                "BS",
                                Object::Dictionary(dictionary! {
                                    "W" => Object::Real(b.width),
                                    "S" => Object::Name(b"S".to_vec())
                                }),
                            );
                        }
                    }
                    if mk.len() > 0 {
                        field_dict.set("MK", Object::Dictionary(mk));
                    }

                    let field_id = doc.add_object(Object::Dictionary(field_dict));
                    acro_fields.push(Object::Reference(field_id));
                    page_annots[*page].push(field_id);
                }
                FieldDef::CheckBox {
                    name,
                    page,
                    x,
                    y,
                    size,
                    checked,
                    on_value,
                    required,
                    read_only,
                    tooltip,
                    border,
                    background,
                } => {
                    let on = on_value.clone().unwrap_or_else(|| "Yes".to_string());

                    let off_id = doc.add_object(Object::Stream(button_off_appearance(*size)));
                    let on_id = doc.add_object(Object::Stream(checkbox_on_appearance(*size)));

                    let mut ap_n = Dictionary::new();
                    ap_n.set(on.as_bytes().to_vec(), Object::Reference(on_id));
                    ap_n.set("Off", Object::Reference(off_id));

                    let as_val = if *checked {
                        Object::Name(on.as_bytes().to_vec())
                    } else {
                        Object::Name(b"Off".to_vec())
                    };
                    let v_val = as_val.clone();

                    let flags: i64 = ((*read_only as i64) << 0) | ((*required as i64) << 1);

                    let rect = Object::Array(vec![
                        Object::Real(*x),
                        Object::Real(*y),
                        Object::Real(*x + *size),
                        Object::Real(*y + *size),
                    ]);

                    let mut field_dict = Dictionary::new();
                    field_dict.set("Type", Object::Name(b"Annot".to_vec()));
                    field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
                    field_dict.set("FT", Object::Name(b"Btn".to_vec()));
                    field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
                    field_dict.set("Rect", rect);
                    field_dict.set("V", v_val);
                    field_dict.set("AS", as_val);
                    field_dict.set("Ff", Object::Integer(flags));
                    field_dict.set(
                        "AP",
                        Object::Dictionary(dictionary! {
                            "N" => Object::Dictionary(ap_n)
                        }),
                    );
                    field_dict.set("P", Object::Reference(page_ids[*page]));

                    if let Some(tip) = tooltip {
                        if !tip.is_empty() {
                            field_dict.set("TU", Object::string_literal(tip.as_bytes().to_vec()));
                        }
                    }

                    let mut mk = Dictionary::new();
                    if let Some(bg) = background {
                        mk.set(
                            "BG",
                            Object::Array(vec![
                                Object::Real(bg[0]),
                                Object::Real(bg[1]),
                                Object::Real(bg[2]),
                            ]),
                        );
                    }
                    if let Some(b) = border {
                        mk.set(
                            "BC",
                            Object::Array(vec![
                                Object::Real(b.color[0]),
                                Object::Real(b.color[1]),
                                Object::Real(b.color[2]),
                            ]),
                        );
                        if (b.width - 1.0).abs() > 0.001 {
                            field_dict.set(
                                "BS",
                                Object::Dictionary(dictionary! {
                                    "W" => Object::Real(b.width),
                                    "S" => Object::Name(b"S".to_vec())
                                }),
                            );
                        }
                    }
                    if mk.len() > 0 {
                        field_dict.set("MK", Object::Dictionary(mk));
                    }

                    let field_id = doc.add_object(Object::Dictionary(field_dict));
                    acro_fields.push(Object::Reference(field_id));
                    page_annots[*page].push(field_id);
                }
                FieldDef::RadioGroup {
                    name,
                    selected,
                    required,
                    read_only,
                    tooltip,
                    options,
                } => {
                    let parent_id = doc.new_object_id();

                    let v_val = if let Some(sel) = selected {
                        Object::Name(sel.as_bytes().to_vec())
                    } else {
                        Object::Name(b"Off".to_vec())
                    };

                    let flags: i64 = (1_i64 << 15)
                        | ((*read_only as i64) << 0)
                        | ((*required as i64) << 1);

                    let mut kids_refs: Vec<Object> = Vec::new();

                    for opt in options {
                        let off_id =
                            doc.add_object(Object::Stream(button_off_appearance(opt.size)));
                        let on_id =
                            doc.add_object(Object::Stream(radio_on_appearance(opt.size)));

                        let mut ap_n = Dictionary::new();
                        ap_n.set(opt.value.as_bytes().to_vec(), Object::Reference(on_id));
                        ap_n.set("Off", Object::Reference(off_id));

                        let is_selected = selected
                            .as_ref()
                            .map(|s| s == &opt.value)
                            .unwrap_or(false);
                        let as_val = if is_selected {
                            Object::Name(opt.value.as_bytes().to_vec())
                        } else {
                            Object::Name(b"Off".to_vec())
                        };

                        let rect = Object::Array(vec![
                            Object::Real(opt.x),
                            Object::Real(opt.y),
                            Object::Real(opt.x + opt.size),
                            Object::Real(opt.y + opt.size),
                        ]);

                        let mut kid_dict = Dictionary::new();
                        kid_dict.set("Type", Object::Name(b"Annot".to_vec()));
                        kid_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
                        kid_dict.set("Rect", rect);
                        kid_dict.set("Parent", Object::Reference(parent_id));
                        kid_dict.set("P", Object::Reference(page_ids[opt.page]));
                        kid_dict.set("AS", as_val);
                        kid_dict.set(
                            "AP",
                            Object::Dictionary(dictionary! {
                                "N" => Object::Dictionary(ap_n)
                            }),
                        );

                        let kid_id = doc.add_object(Object::Dictionary(kid_dict));
                        kids_refs.push(Object::Reference(kid_id));
                        page_annots[opt.page].push(kid_id);
                    }

                    let mut parent_dict = Dictionary::new();
                    parent_dict.set("FT", Object::Name(b"Btn".to_vec()));
                    parent_dict.set("Ff", Object::Integer(flags));
                    parent_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
                    parent_dict.set("Kids", Object::Array(kids_refs));
                    parent_dict.set("V", v_val);

                    if let Some(tip) = tooltip {
                        if !tip.is_empty() {
                            parent_dict
                                .set("TU", Object::string_literal(tip.as_bytes().to_vec()));
                        }
                    }

                    doc.set_object(parent_id, Object::Dictionary(parent_dict));
                    acro_fields.push(Object::Reference(parent_id));
                }
                FieldDef::Choice {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    combo,
                    options,
                    selected,
                    required,
                    read_only,
                    tooltip,
                    border,
                    background,
                } => {
                    let value = selected.clone().unwrap_or_default();
                    let val_bytes = crate::appearance::encode_winansi(&value);

                    let content = crate::appearance::text_appearance_content(
                        &val_bytes,
                        12.0,
                        *width,
                        *height,
                        0,
                        "0 g",
                        "Helv",
                        &widths,
                    );
                    let ap_stream = crate::appearance::build_appearance_xobject(
                        content, *width, *height, "Helv", helv,
                    );
                    let ap_id = doc.add_object(Object::Stream(ap_stream));

                    let flags: i64 = ((*read_only as i64) << 0)
                        | ((*required as i64) << 1)
                        | ((*combo as i64) << 17);

                    let rect = Object::Array(vec![
                        Object::Real(*x),
                        Object::Real(*y),
                        Object::Real(*x + *width),
                        Object::Real(*y + *height),
                    ]);

                    let opt_array: Vec<Object> = options
                        .iter()
                        .map(|o| Object::string_literal(o.as_bytes().to_vec()))
                        .collect();

                    let mut field_dict = Dictionary::new();
                    field_dict.set("Type", Object::Name(b"Annot".to_vec()));
                    field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
                    field_dict.set("FT", Object::Name(b"Ch".to_vec()));
                    field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
                    field_dict.set("Rect", rect);
                    field_dict.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
                    field_dict.set("Ff", Object::Integer(flags));
                    field_dict.set("Opt", Object::Array(opt_array));
                    field_dict.set("V", Object::string_literal(val_bytes));
                    if let Some(sel) = selected {
                        let idx = options.iter().position(|o| o == sel).unwrap() as i64;
                        field_dict.set("I", Object::Array(vec![Object::Integer(idx)]));
                    }
                    field_dict.set(
                        "AP",
                        Object::Dictionary(dictionary! {
                            "N" => Object::Reference(ap_id)
                        }),
                    );
                    field_dict.set("P", Object::Reference(page_ids[*page]));

                    if let Some(tip) = tooltip {
                        if !tip.is_empty() {
                            field_dict.set("TU", Object::string_literal(tip.as_bytes().to_vec()));
                        }
                    }

                    let mut mk = Dictionary::new();
                    if let Some(bg) = background {
                        mk.set(
                            "BG",
                            Object::Array(vec![
                                Object::Real(bg[0]),
                                Object::Real(bg[1]),
                                Object::Real(bg[2]),
                            ]),
                        );
                    }
                    if let Some(b) = border {
                        mk.set(
                            "BC",
                            Object::Array(vec![
                                Object::Real(b.color[0]),
                                Object::Real(b.color[1]),
                                Object::Real(b.color[2]),
                            ]),
                        );
                        if (b.width - 1.0).abs() > 0.001 {
                            field_dict.set(
                                "BS",
                                Object::Dictionary(dictionary! {
                                    "W" => Object::Real(b.width),
                                    "S" => Object::Name(b"S".to_vec())
                                }),
                            );
                        }
                    }
                    if mk.len() > 0 {
                        field_dict.set("MK", Object::Dictionary(mk));
                    }

                    let field_id = doc.add_object(Object::Dictionary(field_dict));
                    acro_fields.push(Object::Reference(field_id));
                    page_annots[*page].push(field_id);
                }
                FieldDef::Signature {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    required,
                    read_only,
                    tooltip,
                    border,
                    background,
                } => {
                    let flags: i64 =
                        ((*read_only as i64) << 0) | ((*required as i64) << 1);

                    let rect = Object::Array(vec![
                        Object::Real(*x),
                        Object::Real(*y),
                        Object::Real(*x + *width),
                        Object::Real(*y + *height),
                    ]);

                    let mut field_dict = Dictionary::new();
                    field_dict.set("Type", Object::Name(b"Annot".to_vec()));
                    field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
                    field_dict.set("FT", Object::Name(b"Sig".to_vec()));
                    field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
                    field_dict.set("Rect", rect);
                    field_dict.set("Ff", Object::Integer(flags));
                    field_dict.set("P", Object::Reference(page_ids[*page]));

                    if let Some(tip) = tooltip {
                        if !tip.is_empty() {
                            field_dict.set("TU", Object::string_literal(tip.as_bytes().to_vec()));
                        }
                    }

                    let mut mk = Dictionary::new();
                    if let Some(bg) = background {
                        mk.set(
                            "BG",
                            Object::Array(vec![
                                Object::Real(bg[0]),
                                Object::Real(bg[1]),
                                Object::Real(bg[2]),
                            ]),
                        );
                    }
                    if let Some(b) = border {
                        mk.set(
                            "BC",
                            Object::Array(vec![
                                Object::Real(b.color[0]),
                                Object::Real(b.color[1]),
                                Object::Real(b.color[2]),
                            ]),
                        );
                        if (b.width - 1.0).abs() > 0.001 {
                            field_dict.set(
                                "BS",
                                Object::Dictionary(dictionary! {
                                    "W" => Object::Real(b.width),
                                    "S" => Object::Name(b"S".to_vec())
                                }),
                            );
                        }
                    }
                    if mk.len() > 0 {
                        field_dict.set("MK", Object::Dictionary(mk));
                    }

                    let field_id = doc.add_object(Object::Dictionary(field_dict));
                    acro_fields.push(Object::Reference(field_id));
                    page_annots[*page].push(field_id);
                }
            }
        }

        // Append widget annotations to their respective pages
        for (pg_idx, annot_ids) in page_annots.iter().enumerate() {
            if annot_ids.is_empty() {
                continue;
            }
            let page_obj = doc
                .get_object_mut(page_ids[pg_idx])
                .expect("page must exist");
            let page_dict = page_obj.as_dict_mut().expect("page must be a dict");
            let annots = page_dict
                .get_mut(b"Annots")
                .ok()
                .and_then(|o| if let Object::Array(_) = o { Some(o) } else { None });
            if let Some(Object::Array(arr)) = annots {
                for &aid in annot_ids {
                    arr.push(Object::Reference(aid));
                }
            } else {
                let arr: Vec<Object> =
                    annot_ids.iter().map(|&aid| Object::Reference(aid)).collect();
                page_dict.set("Annots", Object::Array(arr));
            }
        }

        // Build and add AcroForm
        let acro_dict = dictionary! {
            "Fields" => Object::Array(acro_fields),
            "DR" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "Helv" => Object::Reference(helv)
                })
            }),
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
            "NeedAppearances" => Object::Boolean(false)
        };
        let acro_id = doc.add_object(Object::Dictionary(acro_dict));
        Some(acro_id)
    } else {
        None
    };

    let mut catalog_dict = dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    };
    if let Some(acro_id) = acro_form_ref {
        catalog_dict.set("AcroForm", Object::Reference(acro_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(catalog_dict));
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
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"Pages"));
    }

    #[test]
    fn page_has_mediabox() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], "[]").unwrap();
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
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#, &[], "[]").unwrap();
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
        let out = create_document_json(r#"[{"op":"addPage","width":100,"height":200},{"op":"addPage","width":300,"height":400}]"#, &[], "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(pages.len(), 2);
        let p0 = doc.get_dictionary(pages[0].1).unwrap();
        let mb0 = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb0[2].as_float().unwrap() - 100.0).abs() < 0.5);
    }

    #[test]
    fn errors_on_no_pages() {
        let r = create_document_json(r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#, &[], "[]");
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_text_page_out_of_range() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":1,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#, &[], "[]");
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#, &[], "[]");
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn output_parses_and_is_nonempty() {
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], "[]").unwrap();
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
        let out = create_document_json(&json, png, "[]").unwrap();
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
        let r = create_document_json(&json, png, "[]");
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
        let r = create_document_json(&json, png, "[]");
        assert!(r.unwrap_err().contains("out of bounds"));
    }

    #[test]
    fn image_info_via_signature_image() {
        let img = crate::appearance::signature_image(tiny_png()).unwrap();
        let info = img.info();
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
    }

    #[test]
    fn creates_doc_with_rectangle() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"rectangle","page":0,"x":50,"y":100,"width":200,"height":80,"color":[0.9,0.9,0.9],"borderColor":[0,0,0],"borderWidth":1}]"#,
            &[],
            "[]",
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains(" re"), "content missing 're' operator: {s}");
        assert!(s.contains("B"), "content missing 'B' paint operator: {s}");
    }

    #[test]
    fn creates_doc_with_opacity() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"rectangle","page":0,"x":50,"y":100,"width":200,"height":80,"color":[0.9,0.9,0.9],"opacity":0.5}]"#,
            &[],
            "[]",
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let extgstate = res.get(b"ExtGState").unwrap().as_dict().unwrap();
        let bpg0_ref = extgstate.get(b"BPG0").expect("BPG0 not found in ExtGState");
        let bpg0_id = bpg0_ref.as_reference().unwrap();
        let bpg0_dict = doc.get_object(bpg0_id).unwrap().as_dict().unwrap().clone();
        let ca = bpg0_dict.get(b"ca").unwrap();
        let ca_val = match ca {
            lopdf::Object::Real(v) => *v,
            lopdf::Object::Integer(v) => *v as f32,
            _ => panic!("ca is not a number"),
        };
        assert!(
            (ca_val - 0.5).abs() < 0.01,
            "expected ca ~= 0.5, got {ca_val}"
        );
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains("/BPG0 gs"), "content missing '/BPG0 gs': {s}");
    }

    #[test]
    fn creates_doc_with_line_and_ellipse() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"line","page":0,"x1":50,"y1":100,"x2":250,"y2":100,"thickness":2,"color":[1,0,0]},{"op":"ellipse","page":0,"x":150,"y":400,"xScale":100,"yScale":40,"color":[0,0,1],"borderColor":[0,0,0],"borderWidth":1}]"#,
            &[],
            "[]",
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(s.contains(" l"), "content missing 'l' operator: {s}");
        assert!(s.contains("S"), "content missing 'S' paint operator: {s}");
        assert!(
            s.matches(" c").count() >= 4,
            "expected >= 4 cubic bezier segments for ellipse: {s}"
        );
    }

    #[test]
    fn creates_text_field() {
        let fields = r#"[{"type":"text","name":"fullName","page":0,"x":56,"y":700,"width":200,"height":20,"value":"Ada"}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert!(doc.catalog().unwrap().has(b"AcroForm"));
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("fullName"), "json: {json}");
        assert!(json.contains("Ada"), "json: {json}");
    }

    #[test]
    fn text_field_on_page_annots() {
        let fields = r#"[{"type":"text","name":"a","page":0,"x":10,"y":10,"width":100,"height":20}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], fields).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(doc.get_dictionary(pid).unwrap().get(b"Annots").unwrap().as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_duplicate_field_name() {
        let f = r#"[{"type":"text","name":"x","page":0,"x":0,"y":0,"width":10,"height":10},{"type":"text","name":"x","page":0,"x":0,"y":40,"width":10,"height":10}]"#;
        assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).is_err());
    }

    #[test]
    fn rejects_field_bad_page() {
        let f = r#"[{"type":"text","name":"x","page":5,"x":0,"y":0,"width":10,"height":10}]"#;
        assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).is_err());
    }

    #[test]
    fn creates_checkbox_checked() {
        let f = r#"[{"type":"checkBox","name":"agree","page":0,"x":56,"y":660,"size":14,"checked":true}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("agree") && json.contains("\"type\":\"checkbox\""));
        assert!(json.contains("Yes"));
    }

    #[test]
    fn checkbox_custom_on_value() {
        let f = r#"[{"type":"checkBox","name":"c","page":0,"x":0,"y":0,"size":12,"onValue":"On"}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        assert!(crate::forms::read_fields_json(&out).unwrap().contains("On"));
    }

    #[test]
    fn creates_radio_group() {
        let f = r#"[{"type":"radioGroup","name":"plan","selected":"pro","options":[{"value":"free","page":0,"x":56,"y":620,"size":14},{"value":"pro","page":0,"x":56,"y":600,"size":14}]}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("\"type\":\"radio\""));
        assert!(json.contains("free") && json.contains("pro"));
        // parent in /Fields, 2 kids in page Annots
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(doc.get_dictionary(pid).unwrap().get(b"Annots").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn radio_rejects_unknown_selected() {
        let f = r#"[{"type":"radioGroup","name":"p","selected":"nope","options":[{"value":"a","page":0,"x":0,"y":0,"size":12}]}]"#;
        assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).is_err());
    }

    #[test]
    fn radio_rejects_empty_options() {
        let f = r#"[{"type":"radioGroup","name":"p","options":[]}]"#;
        assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).is_err());
    }

    #[test]
    fn creates_dropdown() {
        let f = r#"[{"type":"choice","name":"country","page":0,"x":56,"y":560,"width":120,"height":20,"combo":true,"options":["AR","BR","CL"],"selected":"AR"}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("\"type\":\"dropdown\""), "json: {json}");
        assert!(json.contains("AR") && json.contains("BR") && json.contains("CL"), "json: {json}");
    }

    #[test]
    fn creates_listbox() {
        let f = r#"[{"type":"choice","name":"langs","page":0,"x":56,"y":500,"width":120,"height":50,"combo":false,"options":["es","pt"]}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        assert!(crate::forms::read_fields_json(&out).unwrap().contains("\"type\":\"listbox\""));
    }

    #[test]
    fn choice_rejects_unknown_selected() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"options":["a"],"selected":"z"}]"#;
        assert!(create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).is_err());
    }

    #[test]
    fn creates_signature_field() {
        let f = r#"[{"type":"signature","name":"sig","page":0,"x":300,"y":560,"width":160,"height":60}]"#;
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842}]"#, &[], f).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("\"type\":\"signature\"") && json.contains("sig"), "json: {json}");
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(doc.get_dictionary(pid).unwrap().get(b"Annots").unwrap().as_array().unwrap().len(), 1);
    }
}
