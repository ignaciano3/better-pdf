//! Build a new PDF document from scratch (pages + text + images), reusing the
//! text and image emission helpers from the draw engine.

use lopdf::{Dictionary, Document, Object, Stream, dictionary};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write;

use crate::draw::{
    STANDARD_14, Seg, check_color, check_finite, check_opacity, check_page, emit_ellipse,
    emit_image_op, emit_line, emit_path, emit_rectangle, emit_text_block, emit_text_block_cid,
    extgstate_dict, font_dict, link_annot_dict, standard_14_index,
};
use crate::fonts::{BuiltFont, EmbeddedFontInput, build_embedded_font};
use lopdf::ObjectId;

fn default_true() -> bool {
    true
}

/// One embedded font descriptor in the `fonts_json` payload. `offset`/`length`
/// index into the concatenated `fonts` blob; `subset` controls glyph subsetting.
#[derive(Deserialize)]
pub(crate) struct FontDesc {
    pub(crate) offset: usize,
    pub(crate) length: usize,
    #[serde(default = "default_true")]
    pub(crate) subset: bool,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CreateOp {
    AddPage {
        width: f32,
        height: f32,
    },
    Text {
        page: usize,
        x: f32,
        y: f32,
        size: f32,
        #[serde(default)]
        font: String,
        #[serde(default, rename = "fontId")]
        font_id: Option<usize>,
        color: [f32; 3],
        text: String,
        #[serde(rename = "lineHeight")]
        line_height: Option<f32>,
        #[serde(default)]
        rotate: Option<f32>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default, rename = "maxWidth")]
        max_width: Option<f32>,
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
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        rotate: f32,
        #[serde(rename = "xSkew", default)]
        x_skew: f32,
        #[serde(rename = "ySkew", default)]
        y_skew: f32,
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
        #[serde(default)]
        dash: Vec<f32>,
        #[serde(rename = "dashPhase", default)]
        dash_phase: f32,
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
        #[serde(default)]
        dash: Vec<f32>,
        #[serde(rename = "dashPhase", default)]
        dash_phase: f32,
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
        #[serde(default)]
        dash: Vec<f32>,
        #[serde(rename = "dashPhase", default)]
        dash_phase: f32,
    },
    Page {
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(rename = "imageOffset")]
        image_offset: usize,
        #[serde(rename = "imageLength")]
        image_length: usize,
        #[serde(rename = "srcPage")]
        src_page: usize,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        rotate: f32,
        #[serde(rename = "xSkew", default)]
        x_skew: f32,
        #[serde(rename = "ySkew", default)]
        y_skew: f32,
    },
    SetRotation {
        page: usize,
        degrees: i64,
    },
    SetMediaBox {
        page: usize,
        #[serde(rename = "box")]
        media_box: [f32; 4],
    },
    Link {
        page: usize,
        rect: [f32; 4],
        uri: Option<String>,
        #[serde(rename = "goToPage")]
        go_to_page: Option<usize>,
    },
    /// Set document-level Info dictionary. If multiple metadata ops are
    /// present, the last one wins.
    Metadata {
        #[serde(flatten)]
        meta: crate::metadata::Metadata,
    },
    Path {
        page: usize,
        segments: Vec<Seg>,
        fill: Option<[f32; 3]>,
        stroke: Option<[f32; 3]>,
        #[serde(rename = "strokeWidth")]
        stroke_width: Option<f32>,
        opacity: Option<f32>,
        #[serde(default)]
        dash: Vec<f32>,
        #[serde(rename = "dashPhase", default)]
        dash_phase: f32,
    },
    /// Document outline (bookmarks). If multiple outline ops are present, the
    /// last one wins.
    Outline {
        items: Vec<crate::outline::OutlineItem>,
    },
}

#[derive(Deserialize)]
pub(crate) struct Border {
    color: [f32; 3],
    width: f32,
}

/// Set a widget's tooltip (`/TU`) when a non-empty tooltip is provided.
fn apply_tooltip(widget: &mut Dictionary, tooltip: &Option<String>) {
    if let Some(tip) = tooltip
        && !tip.is_empty()
    {
        widget.set("TU", Object::string_literal(tip.as_bytes().to_vec()));
    }
}

/// Apply a field's background/border options to a widget: the appearance
/// characteristics dict `/MK` (background `BG`, border color `BC`) and, when the
/// border width differs from the default, the border style dict `/BS`.
fn apply_mk_and_border(
    widget: &mut Dictionary,
    background: &Option<[f32; 3]>,
    border: &Option<Border>,
) {
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
            widget.set(
                "BS",
                Object::Dictionary(dictionary! {
                    "W" => Object::Real(b.width),
                    "S" => Object::Name(b"S".to_vec())
                }),
            );
        }
    }
    if !mk.is_empty() {
        widget.set("MK", Object::Dictionary(mk));
    }
}

/// Build a PDF color operator for a field's text (`/DA` color and appearance
/// content). RGB -> `"r g b rg"`; `None` -> black `"0 g"`.
fn color_op(c: Option<[f32; 3]>) -> String {
    match c {
        Some([r, g, b]) => format!("{r} {g} {b} rg"),
        None => "0 g".to_string(),
    }
}

/// Map a field's `align` string to a PDF quadding value (`/Q`):
/// `"center"` -> 1, `"right"` -> 2, anything else (incl. `None`) -> 0 (left).
fn quadding(align: &Option<String>) -> i64 {
    match align.as_deref() {
        Some("center") => 1,
        Some("right") => 2,
        _ => 0,
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum FieldDef {
    Text {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        value: Option<String>,
        default_value: Option<String>,
        max_length: Option<i64>,
        multiline: Option<bool>,
        #[serde(default)]
        password: bool,
        #[serde(default)]
        comb: bool,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
        text_color: Option<[f32; 3]>,
        font_size: Option<f32>,
        align: Option<String>,
        #[serde(default)]
        font: Option<String>,
        #[serde(default)]
        font_id: Option<usize>,
    },
    CheckBox {
        name: String,
        page: usize,
        x: f32,
        y: f32,
        size: f32,
        #[serde(default)]
        checked: bool,
        default_checked: Option<bool>,
        on_value: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
        check_style: Option<String>,
    },
    RadioGroup {
        name: String,
        selected: Option<String>,
        default_selected: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        read_only: bool,
        tooltip: Option<String>,
        options: Vec<RadioOption>,
        check_style: Option<String>,
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
        #[serde(default)]
        editable: bool,
        #[serde(default)]
        multiselect: bool,
        options: Vec<String>,
        selected: Option<String>,
        default_selected: Option<String>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
        text_color: Option<[f32; 3]>,
        font_size: Option<f32>,
        align: Option<String>,
        #[serde(default)]
        font: Option<String>,
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
        #[serde(default)]
        read_only: bool,
        tooltip: Option<String>,
        border: Option<Border>,
        background: Option<[f32; 3]>,
    },
}

#[derive(Deserialize)]
pub(crate) struct RadioOption {
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

/// On appearance for a button (checkbox or radio) given a mark `style`.
/// Supported: `"check"` (tick), `"cross"` (X), `"circle"` (filled dot),
/// `"square"`, `"diamond"`, `"star"`. Unknown styles fall back to `"check"`.
fn mark_on_appearance(style: &str, size: f32) -> Stream {
    use crate::draw::fmt_num;
    let content: Vec<u8> = match style {
        "circle" => return radio_on_appearance(size),
        "cross" => {
            let p = size * 0.22;
            let t = size * 0.12;
            format!(
                "q {} w 0 0 0 RG {} {} m {} {} l {} {} m {} {} l S Q",
                fmt_num(t),
                fmt_num(p),
                fmt_num(p),
                fmt_num(size - p),
                fmt_num(size - p),
                fmt_num(p),
                fmt_num(size - p),
                fmt_num(size - p),
                fmt_num(p),
            )
            .into_bytes()
        }
        "square" => {
            let p = size * 0.28;
            let s = size - 2.0 * p;
            format!(
                "q 0 0 0 rg {} {} {} {} re f Q",
                fmt_num(p),
                fmt_num(p),
                fmt_num(s),
                fmt_num(s)
            )
            .into_bytes()
        }
        "diamond" => {
            let c = size / 2.0;
            let r = size * 0.32;
            format!(
                "q 0 0 0 rg {} {} m {} {} l {} {} l {} {} l f Q",
                fmt_num(c),
                fmt_num(c + r),
                fmt_num(c + r),
                fmt_num(c),
                fmt_num(c),
                fmt_num(c - r),
                fmt_num(c - r),
                fmt_num(c),
            )
            .into_bytes()
        }
        "star" => {
            let c = size / 2.0;
            let outer = size * 0.45;
            let inner = outer * 0.382;
            let mut s = String::from("q 0 0 0 rg ");
            for i in 0..10 {
                let r = if i % 2 == 0 { outer } else { inner };
                let ang = std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::PI / 5.0;
                let px = c + r * ang.cos();
                let py = c + r * ang.sin();
                let op = if i == 0 { "m" } else { "l" };
                s.push_str(&format!("{} {} {} ", fmt_num(px), fmt_num(py), op));
            }
            s.push_str("f Q");
            s.into_bytes()
        }
        _ => return checkbox_on_appearance(size),
    };
    button_xobject(size, content)
}

/// Map a standard-14 base font name to its deterministic AcroForm /DR resource
/// alias. Returns `None` for names that are not standard-14 text fonts.
pub(crate) fn da_font_alias(base: &str) -> Option<&'static str> {
    Some(match base {
        "Helvetica" => "Helv",
        "Helvetica-Bold" => "HeBo",
        "Helvetica-Oblique" => "HeOb",
        "Helvetica-BoldOblique" => "HeBO",
        "Courier" => "Cour",
        "Courier-Bold" => "CoBo",
        "Courier-Oblique" => "CoOb",
        "Courier-BoldOblique" => "CoBO",
        "Times-Roman" => "TiRo",
        "Times-Bold" => "TiBo",
        "Times-Italic" => "TiIt",
        "Times-BoldItalic" => "TiBI",
        _ => return None,
    })
}

pub fn create_document_json(
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    fields_json: &str,
) -> Result<Vec<u8>, String> {
    let ops: Vec<CreateOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid create ops: {e}"))?;

    let font_descs: Vec<FontDesc> =
        serde_json::from_str(fonts_json).map_err(|e| format!("invalid fonts: {e}"))?;

    // Validate font descriptor byte ranges up front.
    for fd in &font_descs {
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        if end > fonts.len() {
            return Err("font range out of bounds".to_string());
        }
    }

    // Parse fields, treating "" as empty array
    let effective_fields_json = if fields_json.is_empty() {
        "[]"
    } else {
        fields_json
    };
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

    validate_create(&ops, &fields, &font_descs, images, &pages)?;

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // PRE-PASS: build each embedded font once, before the page-building loop.
    // Gather used_chars across ALL text ops referencing each embedded font id,
    // then build the Type0 object graph once. Cache `(type0_id, BuiltFont)`
    // keyed by font id. The shared type0_id is referenced from every page that
    // has a text op for that font.
    let mut embedded_fonts: std::collections::HashMap<usize, (ObjectId, BuiltFont)> =
        std::collections::HashMap::new();
    {
        let mut used_per_font: std::collections::HashMap<usize, std::collections::BTreeSet<char>> =
            std::collections::HashMap::new();
        for op in &ops {
            if let CreateOp::Text {
                font_id: Some(i),
                text,
                ..
            } = op
            {
                used_per_font.entry(*i).or_default().extend(text.chars());
            }
        }
        for field in &fields {
            if let FieldDef::Text {
                font_id: Some(i),
                value,
                default_value,
                ..
            } = field
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
        // Deterministic build order by font id.
        let mut ids: Vec<usize> = used_per_font.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let fd = &font_descs[id];
            let bytes = &fonts[fd.offset..fd.offset + fd.length];
            let input = EmbeddedFontInput {
                data: bytes,
                subset: fd.subset,
                used_chars: used_per_font.remove(&id).unwrap_or_default(),
            };
            let mut add = |o: Object| doc.add_object(o);
            let built = build_embedded_font(&mut add, &input)?;
            embedded_fonts.insert(id, built);
        }
    }

    let page_ids = build_pages(
        &mut doc,
        pages_id,
        &ops,
        &pages,
        &embedded_fonts,
        &font_descs,
        fonts,
        images,
    )?;

    // Build link annotations and append them to their pages' /Annots. This is
    // independent of form fields, so it runs for documents with no AcroForm.
    for op in &ops {
        if let CreateOp::Link {
            page,
            rect,
            uri,
            go_to_page,
        } = op
        {
            let dest_page = go_to_page.map(|t| page_ids[t]);
            let annot = link_annot_dict(*rect, uri.as_deref(), dest_page);
            let annot_id = doc.add_object(Object::Dictionary(annot));
            let page_obj = doc
                .get_object_mut(page_ids[*page])
                .map_err(|e| format!("internal: page object {:?} missing: {e}", page_ids[*page]))?;
            let page_dict = page_obj
                .as_dict_mut()
                .map_err(|e| format!("internal: page object is not a dict: {e}"))?;
            match page_dict.get_mut(b"Annots") {
                Ok(Object::Array(arr)) => arr.push(Object::Reference(annot_id)),
                _ => page_dict.set("Annots", Object::Array(vec![Object::Reference(annot_id)])),
            }
        }
    }

    let acro_form_ref = build_fields_and_acroform(
        &mut doc,
        &fields,
        &page_ids,
        &embedded_fonts,
        &font_descs,
        fonts,
    )?;

    let mut catalog_dict = dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    };
    if let Some(acro_id) = acro_form_ref {
        catalog_dict.set("AcroForm", Object::Reference(acro_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(catalog_dict));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    // Build the document outline if an outline op was present (last one wins),
    // then wire /Outlines onto the catalog.
    let outline_items = ops
        .iter()
        .filter_map(|o| {
            if let CreateOp::Outline { items } = o {
                Some(items)
            } else {
                None
            }
        })
        .next_back();
    if let Some(items) = outline_items {
        let root = crate::outline::build_outline(&mut doc, items, &|i| page_ids.get(i).copied())?;
        let catalog = doc
            .get_object_mut(catalog_id)
            .and_then(Object::as_dict_mut)
            .map_err(|e| e.to_string())?;
        catalog.set("Outlines", Object::Reference(root));
    }

    // Apply metadata if a metadata op was present (last one wins).
    let meta_op = ops
        .iter()
        .filter_map(|o| {
            if let CreateOp::Metadata { meta } = o {
                Some(meta)
            } else {
                None
            }
        })
        .next_back();
    if let Some(meta) = meta_op {
        let info_id = doc.add_object(Object::Dictionary(crate::metadata::build_info_dict(meta)));
        doc.trailer.set("Info", Object::Reference(info_id));
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Validate every create op and field definition before any object is built,
/// so an invalid request fails without mutating a partially-built document.
fn validate_create(
    ops: &[CreateOp],
    fields: &[FieldDef],
    font_descs: &[FontDesc],
    images: &[u8],
    pages: &[(f32, f32)],
) -> Result<(), String> {
    // Validation pass: check all ops before building anything
    for op in ops {
        match op {
            CreateOp::Text {
                page,
                font,
                font_id,
                opacity,
                rotate,
                max_width,
                ..
            } => {
                check_page(*page, pages.len())?;
                if let Some(i) = font_id {
                    if *i >= font_descs.len() {
                        return Err(format!("font id {i} out of range"));
                    }
                } else if standard_14_index(font).is_none() {
                    return Err(format!("unknown font: {font}"));
                }
                check_opacity(opacity)?;
                if let Some(deg) = rotate
                    && !deg.is_finite()
                {
                    return Err("invalid rotation".to_string());
                }
                if let Some(mw) = max_width
                    && (!mw.is_finite() || *mw <= 0.0)
                {
                    return Err("maxWidth must be > 0".to_string());
                }
            }
            CreateOp::Image {
                page,
                image_offset,
                image_length,
                opacity,
                ..
            } => {
                check_page(*page, pages.len())?;
                check_opacity(opacity)?;
                let end = image_offset
                    .checked_add(*image_length)
                    .ok_or_else(|| "image range out of bounds".to_string())?;
                if end > images.len() {
                    return Err("image range out of bounds".to_string());
                }
                crate::appearance::signature_image(&images[*image_offset..end])?;
            }
            CreateOp::Page {
                page,
                width,
                height,
                image_offset,
                image_length,
                opacity,
                ..
            } => {
                check_page(*page, pages.len())?;
                let end = image_offset
                    .checked_add(*image_length)
                    .ok_or_else(|| "page source range out of bounds".to_string())?;
                if end > images.len() {
                    return Err("page source range out of bounds".to_string());
                }
                if !width.is_finite() || *width <= 0.0 {
                    return Err("width must be finite and > 0".to_string());
                }
                if !height.is_finite() || *height <= 0.0 {
                    return Err("height must be finite and > 0".to_string());
                }
                check_opacity(opacity)?;
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
                ..
            } => {
                check_page(*page, pages.len())?;
                check_opacity(opacity)?;
                if let Some(t) = thickness
                    && (!t.is_finite() || *t < 0.0)
                {
                    return Err("thickness must be >= 0".to_string());
                }
                check_finite(&[*x1, *y1, *x2, *y2], "invalid coordinate")?;
                check_color(color)?;
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
                ..
            } => {
                check_page(*page, pages.len())?;
                check_opacity(opacity)?;
                if let Some(bw) = border_width
                    && (!bw.is_finite() || *bw < 0.0)
                {
                    return Err("borderWidth must be >= 0".to_string());
                }
                check_finite(&[*x, *y, *width, *height], "invalid coordinate")?;
                if *width <= 0.0 {
                    return Err("width must be > 0".to_string());
                }
                if *height <= 0.0 {
                    return Err("height must be > 0".to_string());
                }
                check_color(color)?;
                check_color(border_color)?;
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
                ..
            } => {
                check_page(*page, pages.len())?;
                check_opacity(opacity)?;
                if let Some(bw) = border_width
                    && (!bw.is_finite() || *bw < 0.0)
                {
                    return Err("borderWidth must be >= 0".to_string());
                }
                check_finite(&[*x, *y, *x_scale, *y_scale], "invalid coordinate")?;
                if *x_scale <= 0.0 {
                    return Err("xScale must be > 0".to_string());
                }
                if *y_scale <= 0.0 {
                    return Err("yScale must be > 0".to_string());
                }
                check_color(color)?;
                check_color(border_color)?;
            }
            CreateOp::SetRotation { page, degrees } => {
                check_page(*page, pages.len())?;
                if degrees.rem_euclid(90) != 0 {
                    return Err("degrees must be a multiple of 90".to_string());
                }
            }
            CreateOp::SetMediaBox { page, media_box } => {
                check_page(*page, pages.len())?;
                check_finite(media_box, "invalid media box")?;
                if media_box[2] <= media_box[0] || media_box[3] <= media_box[1] {
                    return Err("invalid media box".to_string());
                }
            }
            CreateOp::Link {
                page,
                rect,
                uri,
                go_to_page,
            } => {
                check_page(*page, pages.len())?;
                match (uri.is_some(), go_to_page.is_some()) {
                    (true, true) | (false, false) => {
                        return Err("link must have exactly one of uri or goToPage".to_string());
                    }
                    _ => {}
                }
                check_finite(rect, "invalid link rect")?;
                if rect[2] <= rect[0] || rect[3] <= rect[1] {
                    return Err("invalid link rect".to_string());
                }
                if let Some(target) = go_to_page
                    && *target >= pages.len()
                {
                    return Err(format!(
                        "goToPage {target} out of range ({} pages)",
                        pages.len()
                    ));
                }
            }
            CreateOp::AddPage { .. } => {}
            CreateOp::Metadata { .. } => {}
            CreateOp::Outline { items } => {
                crate::outline::validate_pages(items, pages.len())?;
            }
            CreateOp::Path {
                page,
                segments,
                fill,
                stroke,
                stroke_width,
                opacity,
                ..
            } => {
                check_page(*page, pages.len())?;
                if segments.is_empty() {
                    return Err("path must have at least one segment".to_string());
                }
                check_opacity(opacity)?;
                if let Some(sw) = stroke_width
                    && (!sw.is_finite() || *sw < 0.0)
                {
                    return Err("strokeWidth must be >= 0".to_string());
                }
                for seg in segments.iter() {
                    let coords: Vec<f32> = match seg {
                        Seg::M { x, y } | Seg::L { x, y } => vec![*x, *y],
                        Seg::C {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => vec![*x1, *y1, *x2, *y2, *x, *y],
                        Seg::Z => vec![],
                    };
                    for &v in &coords {
                        if !v.is_finite() {
                            return Err("invalid coordinate".to_string());
                        }
                    }
                }
                check_color(fill)?;
                check_color(stroke)?;
            }
        }
    }

    validate_fields(fields, pages.len(), font_descs)
}

/// Per-field validation shared by the create and inject paths: empty/duplicate
/// names, per-option radio page range, finite coordinates, positive
/// size/width/height, option-value rules, embedded `font_id` range, and
/// selected/defaultSelected membership — for all five field variants.
/// `page_count` bounds the pages fields may target; `font_descs` bounds
/// embedded `font_id`s. Validation-only: no objects are built.
pub(crate) fn validate_fields(
    fields: &[FieldDef],
    page_count: usize,
    font_descs: &[FontDesc],
) -> Result<(), String> {
    {
        let mut seen_names: HashSet<&str> = HashSet::new();
        for field in fields {
            match field {
                FieldDef::Text {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    max_length,
                    comb,
                    multiline,
                    default_value,
                    font_id,
                    ..
                } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if let Some(i) = font_id
                        && *i >= font_descs.len()
                    {
                        return Err(format!("font id {i} out of range"));
                    }
                    if font_id.is_some() && *comb {
                        return Err("embedded fonts are supported on plain and multiline text fields only".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if let (Some(dv), Some(ml)) = (default_value, max_length)
                        && *ml >= 0
                        && dv.chars().count() as i64 > *ml
                    {
                        return Err(format!(
                            "text field \"{name}\" defaultValue length {} exceeds maxLength {ml}",
                            dv.chars().count()
                        ));
                    }
                    if *page >= page_count {
                        return Err(format!(
                            "field page {page} out of range ({page_count} pages)"
                        ));
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
                    if let Some(ml) = max_length
                        && *ml < 0
                    {
                        return Err("field maxLength must be >= 0".to_string());
                    }
                    if *comb {
                        match max_length {
                            None => return Err("comb field requires maxLength".to_string()),
                            Some(ml) if *ml <= 0 => {
                                return Err("comb field maxLength must be > 0".to_string());
                            }
                            _ => {}
                        }
                        if multiline.unwrap_or(false) {
                            return Err("comb field cannot be multiline".to_string());
                        }
                    }
                }
                FieldDef::CheckBox {
                    name,
                    page,
                    x,
                    y,
                    size,
                    on_value,
                    ..
                } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= page_count {
                        return Err(format!(
                            "field page {page} out of range ({page_count} pages)"
                        ));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("field x/y must be finite".to_string());
                    }
                    if !size.is_finite() || *size <= 0.0 {
                        return Err("checkbox size must be finite and > 0".to_string());
                    }
                    if let Some(ov) = on_value
                        && ov == "Off"
                    {
                        return Err("checkbox onValue must not be \"Off\"".to_string());
                    }
                }
                FieldDef::RadioGroup {
                    name,
                    selected,
                    options,
                    default_selected,
                    ..
                } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if options.is_empty() {
                        return Err(format!(
                            "radioGroup \"{name}\" must have at least one option"
                        ));
                    }
                    if let Some(dv) = default_selected
                        && !options.iter().any(|o| &o.value == dv)
                    {
                        return Err(format!(
                            "radioGroup \"{name}\" defaultSelected value \"{dv}\" is not in options"
                        ));
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
                        if opt.page >= page_count {
                            return Err(format!(
                                "radio option page {} out of range ({page_count} pages)",
                                opt.page
                            ));
                        }
                        if !opt.x.is_finite() || !opt.y.is_finite() {
                            return Err("radio option x/y must be finite".to_string());
                        }
                        if !opt.size.is_finite() || opt.size <= 0.0 {
                            return Err("radio option size must be finite and > 0".to_string());
                        }
                    }
                    if let Some(sel) = selected
                        && !options.iter().any(|o| &o.value == sel)
                    {
                        return Err(format!(
                            "radioGroup \"{name}\" selected value \"{sel}\" is not in options"
                        ));
                    }
                }
                FieldDef::Choice {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    options,
                    selected,
                    default_selected,
                    ..
                } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= page_count {
                        return Err(format!(
                            "field page {page} out of range ({page_count} pages)"
                        ));
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
                        return Err(format!(
                            "choice field \"{name}\" must have at least one option"
                        ));
                    }
                    let mut seen_opts: HashSet<&str> = HashSet::new();
                    for opt in options {
                        if !seen_opts.insert(opt.as_str()) {
                            return Err(format!("duplicate choice option: {opt}"));
                        }
                    }
                    if let Some(sel) = selected
                        && !options.iter().any(|o| o == sel)
                    {
                        return Err(format!(
                            "choice field \"{name}\" selected value \"{sel}\" is not in options"
                        ));
                    }
                    if let Some(dv) = default_selected
                        && !options.iter().any(|o| o == dv)
                    {
                        return Err(format!(
                            "choice field \"{name}\" defaultSelected value \"{dv}\" is not in options"
                        ));
                    }
                }
                FieldDef::Signature {
                    name,
                    page,
                    x,
                    y,
                    width,
                    height,
                    ..
                } => {
                    if name.is_empty() {
                        return Err("field name must not be empty".to_string());
                    }
                    if !seen_names.insert(name.as_str()) {
                        return Err(format!("duplicate field name: {name}"));
                    }
                    if *page >= page_count {
                        return Err(format!(
                            "field page {page} out of range ({page_count} pages)"
                        ));
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
    Ok(())
}

/// Build the page tree: one content stream and `/Page` object per page (in
/// z-order), wire them under `pages_id`, and return the page object ids.
#[allow(clippy::too_many_arguments)]
fn build_pages(
    doc: &mut Document,
    pages_id: ObjectId,
    ops: &[CreateOp],
    pages: &[(f32, f32)],
    embedded_fonts: &std::collections::HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &[FontDesc],
    fonts: &[u8],
    images: &[u8],
) -> Result<Vec<ObjectId>, String> {
    // Global image counter for unique XObject keys
    let mut img_counter: usize = 0;

    // Global page-embed counter for unique BPp Form-XObject keys across all pages
    let mut page_embed_counter: usize = 0;

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
        for op in ops {
            match op {
                CreateOp::Text {
                    page,
                    x,
                    y,
                    size,
                    font,
                    font_id,
                    color,
                    text,
                    line_height,
                    rotate,
                    opacity,
                    max_width,
                } if *page == page_index => {
                    // Register ExtGState for opacity if present
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };

                    // Word-wrap server-side when maxWidth is set (one source of
                    // truth, shared with the loaded-doc draw path).
                    let wrapped: String = match max_width {
                        Some(mw) => {
                            if let Some(id) = font_id {
                                let fd = &font_descs[*id];
                                let fbytes = &fonts[fd.offset..fd.offset + fd.length];
                                crate::fonts::wrap_embedded(fbytes, *size, *mw, text)?
                            } else {
                                crate::appearance::wrap_standard14(text, font, *size, *mw)
                            }
                        }
                        None => text.clone(),
                    };
                    let text = &wrapped;

                    if let Some(id) = font_id {
                        // Embedded font: emit a Type0/Identity-H hex glyph string.
                        // gids come from BuiltFont.gid_for (the REMAPPED subset ids),
                        // NOT from re-deriving via face.glyph_index.
                        let (type0_id, built) = embedded_fonts.get(id).unwrap();
                        let key = format!("BPE{id}");
                        if !font_res.has(key.as_bytes()) {
                            font_res.set(key.clone(), Object::Reference(*type0_id));
                        }
                        let gids_per_line: Vec<Vec<u16>> = text
                            .split('\n')
                            .map(|line| {
                                line.chars()
                                    .filter_map(|c| built.gid_for.get(&c).copied())
                                    .collect()
                            })
                            .collect();
                        emit_text_block_cid(
                            &mut content,
                            &key,
                            *x,
                            *y,
                            *size,
                            *color,
                            &gids_per_line,
                            *line_height,
                            *rotate,
                            gs_key.as_deref(),
                        );
                    } else {
                        let idx = standard_14_index(font).unwrap();
                        // Register font resource if not already added
                        let key = format!("BPF{idx}");
                        if !font_res.has(key.as_bytes()) {
                            let fid =
                                doc.add_object(Object::Dictionary(font_dict(STANDARD_14[idx])));
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
                            *rotate,
                            gs_key.as_deref(),
                        );
                    }
                }
                CreateOp::Image {
                    page,
                    x,
                    y,
                    width,
                    height,
                    image_offset,
                    image_length,
                    opacity,
                    rotate,
                    x_skew,
                    y_skew,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    let end = image_offset + image_length;
                    let img = crate::appearance::signature_image(&images[*image_offset..end])?;
                    let xid =
                        crate::appearance::build_image_xobjects(img, &mut |o| doc.add_object(o));
                    let key = format!("BPI{img_counter}");
                    img_counter += 1;
                    xobject_res.set(key.clone(), Object::Reference(xid));
                    emit_image_op(
                        &mut content,
                        &key,
                        *x,
                        *y,
                        *width,
                        *height,
                        gs_key.as_deref(),
                        *rotate,
                        *x_skew,
                        *y_skew,
                    );
                }
                CreateOp::Page {
                    page,
                    x,
                    y,
                    width,
                    height,
                    image_offset,
                    image_length,
                    src_page,
                    opacity,
                    rotate,
                    x_skew,
                    y_skew,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    let end = image_offset + image_length;
                    let src = &images[*image_offset..end];
                    let (xid, bw, bh) =
                        crate::embed::embed_page_as_xobject(&mut *doc, src, *src_page)?;
                    let key = format!("BPp{page_embed_counter}");
                    page_embed_counter += 1;
                    xobject_res.set(key.clone(), Object::Reference(xid));
                    // Form BBox is [0 0 bw bh], so scale by width/bw, height/bh.
                    content.extend_from_slice(b"q\n");
                    if let Some(k) = gs_key.as_deref() {
                        writeln!(content, "/{k} gs").unwrap();
                    }
                    crate::draw::emit_placement(
                        &mut content,
                        *x,
                        *y,
                        *width / bw,
                        *height / bh,
                        *rotate,
                        *x_skew,
                        *y_skew,
                    );
                    writeln!(content, "/{key} Do").unwrap();
                    content.extend_from_slice(b"Q\n");
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
                    dash,
                    dash_phase,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
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
                        dash,
                        *dash_phase,
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
                    dash,
                    dash_phase,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
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
                        dash,
                        *dash_phase,
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
                    dash,
                    dash_phase,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
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
                        dash,
                        *dash_phase,
                    );
                }
                CreateOp::Path {
                    page,
                    segments,
                    fill,
                    stroke,
                    stroke_width,
                    opacity,
                    dash,
                    dash_phase,
                } if *page == page_index => {
                    let gs_key = if let Some(o) = opacity {
                        let key = format!("BPG{gs_counter}");
                        gs_counter += 1;
                        let gs_id = doc.add_object(Object::Dictionary(extgstate_dict(*o)));
                        extgstate_res.set(key.clone(), Object::Reference(gs_id));
                        Some(key)
                    } else {
                        None
                    };
                    emit_path(
                        &mut content,
                        gs_key.as_deref(),
                        segments,
                        *fill,
                        *stroke,
                        *stroke_width,
                        dash,
                        *dash_phase,
                    );
                }
                CreateOp::SetRotation { .. } => {}
                CreateOp::SetMediaBox { .. } => {}
                _ => {}
            }
        }

        // Build resources dict, only including sub-dicts that have entries
        let mut resources = Dictionary::new();
        if !font_res.is_empty() {
            resources.set("Font", Object::Dictionary(font_res));
        }
        if !xobject_res.is_empty() {
            resources.set("XObject", Object::Dictionary(xobject_res));
        }
        if !extgstate_res.is_empty() {
            resources.set("ExtGState", Object::Dictionary(extgstate_res));
        }

        let content_id = doc.add_object(Object::Stream(Stream::new(
            lopdf::Dictionary::new(),
            content,
        )));
        let mut page_dict = dictionary! {
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
        // Apply page-dict mutation ops (Rotate / MediaBox override).
        for op in ops.iter() {
            match op {
                CreateOp::SetRotation { page, degrees } if *page == page_index => {
                    let norm = ((degrees % 360) + 360) % 360;
                    page_dict.set("Rotate", Object::Integer(norm));
                }
                CreateOp::SetMediaBox { page, media_box } if *page == page_index => {
                    page_dict.set(
                        "MediaBox",
                        Object::Array(media_box.iter().map(|v| Object::Real(*v)).collect()),
                    );
                }
                _ => {}
            }
        }
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
    Ok(page_ids)
}

/// A built field's top-level object id plus which page each of its widget
/// annotations must be appended to. Page indices are 0-based into the caller's
/// page list (create path: fresh pages; inject path: existing pages).
pub(crate) struct BuiltField {
    pub top_field_id: ObjectId,
    pub widgets: Vec<(usize, ObjectId)>,
}

/// Resolved font handle for a text/choice field's appearance stream and /DA.
pub(crate) enum FieldFont<'a> {
    Standard {
        alias: &'a str,
        font_ref: ObjectId,
    },
    Embedded {
        alias: &'a str,
        type0_id: ObjectId,
        built: &'a BuiltFont,
        bytes: &'a [u8],
    },
}

/// Resolve the per-field font handle exactly as the inline create loop did, so
/// aliases (`Helv`, `BPF<n>`) and object references are unchanged. `Text`/
/// `Choice` fields get `Some`; buttons/signatures get `None`.
fn resolve_create_font<'a>(
    field: &FieldDef,
    font_registry: &'a std::collections::HashMap<&str, (&'static str, ObjectId)>,
    embedded_fonts: &'a std::collections::HashMap<usize, (ObjectId, BuiltFont)>,
    embedded_aliases: &'a std::collections::HashMap<usize, String>,
    font_descs: &'a [FontDesc],
    fonts: &'a [u8],
) -> Option<FieldFont<'a>> {
    match field {
        FieldDef::Text {
            font_id: Some(i), ..
        } => {
            let (type0_id, built) = &embedded_fonts[i];
            let fd = &font_descs[*i];
            let bytes = &fonts[fd.offset..fd.offset + fd.length];
            Some(FieldFont::Embedded {
                alias: embedded_aliases[i].as_str(),
                type0_id: *type0_id,
                built,
                bytes,
            })
        }
        FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
            let base = font.as_deref().unwrap_or("Helvetica");
            let (alias, font_ref) = font_registry[base];
            Some(FieldFont::Standard { alias, font_ref })
        }
        _ => None,
    }
}

/// Build one field's object graph (widget/field dict, /AP appearance, radio
/// kids) into `doc`. `font` is `Some` for text/choice fields, `None` otherwise.
/// Returns the top-level field id and the `(page_index, widget_id)` pairs the
/// caller must wire into page `/Annots`. Object add order matches the previous
/// inline construction so create output is byte-identical. Does NOT touch page
/// `/Annots` or the `/AcroForm`.
pub(crate) fn build_one_field(
    doc: &mut Document,
    field: &FieldDef,
    page_ids: &[ObjectId],
    font: Option<FieldFont<'_>>,
) -> Result<BuiltField, String> {
    match field {
        FieldDef::Text {
            name,
            page,
            x,
            y,
            width,
            height,
            value,
            default_value,
            max_length,
            multiline,
            password,
            comb,
            required,
            read_only,
            tooltip,
            border,
            background,
            text_color,
            font_size,
            align,
            font: base_font_name,
            font_id,
        } => {
            let val_str = value.clone().unwrap_or_default();
            let op = color_op(*text_color);
            let size = font_size.unwrap_or(12.0);
            let q = quadding(align);

            let (content, font_alias_str, ap_font_ref) = match font {
                Some(FieldFont::Embedded {
                    alias,
                    type0_id,
                    built,
                    bytes,
                }) => {
                    let content = crate::appearance::text_appearance_content_embedded(
                        &val_str, size, *width, *height, q, &op, alias, built, bytes,
                    );
                    (content, alias.to_string(), type0_id)
                }
                Some(FieldFont::Standard { alias, font_ref }) => {
                    let base_font = base_font_name.as_deref().unwrap_or("Helvetica");
                    let widths = crate::appearance::standard_14_widths(base_font).unwrap();
                    let val_bytes = crate::appearance::encode_winansi(&val_str);
                    let content = if *comb {
                        crate::appearance::text_appearance_content_comb(
                            &val_bytes,
                            size,
                            *width,
                            *height,
                            max_length.unwrap_or(0),
                            &op,
                            alias,
                            &widths,
                        )
                    } else {
                        crate::appearance::text_appearance_content(
                            &val_bytes, size, *width, *height, q, &op, alias, &widths,
                        )
                    };
                    (content, alias.to_string(), font_ref)
                }
                None => unreachable!("text field always resolves a font"),
            };
            let ap_stream = crate::appearance::build_appearance_xobject(
                content,
                *width,
                *height,
                &font_alias_str,
                ap_font_ref,
            );
            let ap_id = doc.add_object(Object::Stream(ap_stream));

            let flags: i64 = (*read_only as i64)
                | ((*required as i64) << 1)
                | ((multiline.unwrap_or(false) as i64) << 12)
                | ((*password as i64) << 13)
                | ((*comb as i64) << 24);

            let rect = Object::Array(vec![
                Object::Real(*x),
                Object::Real(*y),
                Object::Real(*x + *width),
                Object::Real(*y + *height),
            ]);

            let mut field_dict = Dictionary::new();
            field_dict.set("Type", Object::Name(b"Annot".to_vec()));
            field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
            // Print flag (/F bit 3) so the field shows in printed output.
            field_dict.set("F", Object::Integer(4));
            field_dict.set("FT", Object::Name(b"Tx".to_vec()));
            field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
            field_dict.set("Rect", rect);
            field_dict.set(
                "DA",
                Object::string_literal(format!("/{font_alias_str} {size} Tf {op}")),
            );
            if align.is_some() {
                field_dict.set("Q", Object::Integer(q));
            }
            // Embedded-font values may contain characters outside WinAnsi
            // (e.g. CJK), so encode /V and /DV as PDF text strings (which
            // fall back to UTF-16BE) rather than lossy WinAnsi bytes.
            let text_string_obj = |s: &str| {
                if font_id.is_some() {
                    lopdf::text_string(s)
                } else {
                    Object::string_literal(crate::appearance::encode_winansi(s))
                }
            };
            field_dict.set("V", text_string_obj(&val_str));
            if let Some(dv) = default_value {
                field_dict.set("DV", text_string_obj(dv));
            }
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
            apply_tooltip(&mut field_dict, tooltip);

            apply_mk_and_border(&mut field_dict, background, border);

            let field_id = doc.add_object(Object::Dictionary(field_dict));
            Ok(BuiltField {
                top_field_id: field_id,
                widgets: vec![(*page, field_id)],
            })
        }
        FieldDef::CheckBox {
            name,
            page,
            x,
            y,
            size,
            checked,
            default_checked,
            on_value,
            required,
            read_only,
            tooltip,
            border,
            background,
            check_style,
        } => {
            let on = on_value.clone().unwrap_or_else(|| "Yes".to_string());

            let off_id = doc.add_object(Object::Stream(button_off_appearance(*size)));
            let style = check_style.as_deref().unwrap_or("check");
            let on_id = doc.add_object(Object::Stream(mark_on_appearance(style, *size)));

            let mut ap_n = Dictionary::new();
            ap_n.set(on.as_bytes().to_vec(), Object::Reference(on_id));
            ap_n.set("Off", Object::Reference(off_id));

            let as_val = if *checked {
                Object::Name(on.as_bytes().to_vec())
            } else {
                Object::Name(b"Off".to_vec())
            };
            let v_val = as_val.clone();

            let flags: i64 = (*read_only as i64) | ((*required as i64) << 1);

            let rect = Object::Array(vec![
                Object::Real(*x),
                Object::Real(*y),
                Object::Real(*x + *size),
                Object::Real(*y + *size),
            ]);

            let mut field_dict = Dictionary::new();
            field_dict.set("Type", Object::Name(b"Annot".to_vec()));
            field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
            // Print flag (/F bit 3) so the field shows in printed output.
            field_dict.set("F", Object::Integer(4));
            field_dict.set("FT", Object::Name(b"Btn".to_vec()));
            field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
            field_dict.set("Rect", rect);
            field_dict.set("V", v_val);
            field_dict.set("AS", as_val);
            if let Some(dc) = default_checked {
                let dv_name = if *dc {
                    on.as_bytes().to_vec()
                } else {
                    b"Off".to_vec()
                };
                field_dict.set("DV", Object::Name(dv_name));
            }
            field_dict.set("Ff", Object::Integer(flags));
            field_dict.set(
                "AP",
                Object::Dictionary(dictionary! {
                    "N" => Object::Dictionary(ap_n)
                }),
            );
            field_dict.set("P", Object::Reference(page_ids[*page]));

            apply_tooltip(&mut field_dict, tooltip);

            apply_mk_and_border(&mut field_dict, background, border);

            let field_id = doc.add_object(Object::Dictionary(field_dict));
            Ok(BuiltField {
                top_field_id: field_id,
                widgets: vec![(*page, field_id)],
            })
        }
        FieldDef::RadioGroup {
            name,
            selected,
            default_selected,
            required,
            read_only,
            tooltip,
            options,
            check_style,
        } => {
            let parent_id = doc.new_object_id();
            let style = check_style.as_deref().unwrap_or("circle");

            let v_val = if let Some(sel) = selected {
                Object::Name(sel.as_bytes().to_vec())
            } else {
                Object::Name(b"Off".to_vec())
            };

            let flags: i64 = (1_i64 << 15) | (*read_only as i64) | ((*required as i64) << 1);

            let mut kids_refs: Vec<Object> = Vec::new();
            let mut widgets: Vec<(usize, ObjectId)> = Vec::new();

            for opt in options {
                let off_id = doc.add_object(Object::Stream(button_off_appearance(opt.size)));
                let on_id = doc.add_object(Object::Stream(mark_on_appearance(style, opt.size)));

                let mut ap_n = Dictionary::new();
                ap_n.set(opt.value.as_bytes().to_vec(), Object::Reference(on_id));
                ap_n.set("Off", Object::Reference(off_id));

                let is_selected = selected.as_ref().map(|s| s == &opt.value).unwrap_or(false);
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
                // Print flag (/F bit 3) so the button shows in printed output.
                kid_dict.set("F", Object::Integer(4));
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
                widgets.push((opt.page, kid_id));
            }

            let mut parent_dict = Dictionary::new();
            parent_dict.set("FT", Object::Name(b"Btn".to_vec()));
            parent_dict.set("Ff", Object::Integer(flags));
            parent_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
            parent_dict.set("Kids", Object::Array(kids_refs));
            parent_dict.set("V", v_val);
            if let Some(dv) = default_selected {
                parent_dict.set("DV", Object::Name(dv.as_bytes().to_vec()));
            }

            apply_tooltip(&mut parent_dict, tooltip);

            doc.set_object(parent_id, Object::Dictionary(parent_dict));
            Ok(BuiltField {
                top_field_id: parent_id,
                widgets,
            })
        }
        FieldDef::Choice {
            name,
            page,
            x,
            y,
            width,
            height,
            combo,
            editable,
            multiselect,
            options,
            selected,
            default_selected,
            required,
            read_only,
            tooltip,
            border,
            background,
            text_color,
            font_size,
            align,
            font: base_font_name,
        } => {
            let value = selected.clone().unwrap_or_default();
            let val_bytes = crate::appearance::encode_winansi(&value);
            let op = color_op(*text_color);
            let size = font_size.unwrap_or(12.0);
            let q = quadding(align);
            let base_font = base_font_name.as_deref().unwrap_or("Helvetica");
            let (font_alias, font_ref) = match font {
                Some(FieldFont::Standard { alias, font_ref }) => (alias, font_ref),
                _ => unreachable!("choice field always resolves a standard font"),
            };
            let widths = crate::appearance::standard_14_widths(base_font).unwrap();

            let content = crate::appearance::text_appearance_content(
                &val_bytes, size, *width, *height, q, &op, font_alias, &widths,
            );
            let ap_stream = crate::appearance::build_appearance_xobject(
                content, *width, *height, font_alias, font_ref,
            );
            let ap_id = doc.add_object(Object::Stream(ap_stream));

            // Edit flag (bit 18) only meaningful for combo boxes;
            // Multiselect flag (bit 22) only meaningful for list boxes.
            let flags: i64 = (*read_only as i64)
                | ((*required as i64) << 1)
                | ((*combo as i64) << 17)
                | (((*combo && *editable) as i64) << 18)
                | (((!*combo && *multiselect) as i64) << 21);

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
            // Print flag (/F bit 3) so the field shows in printed output.
            field_dict.set("F", Object::Integer(4));
            field_dict.set("FT", Object::Name(b"Ch".to_vec()));
            field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
            field_dict.set("Rect", rect);
            field_dict.set(
                "DA",
                Object::string_literal(format!("/{font_alias} {size} Tf {op}")),
            );
            if align.is_some() {
                field_dict.set("Q", Object::Integer(q));
            }
            field_dict.set("Ff", Object::Integer(flags));
            field_dict.set("Opt", Object::Array(opt_array));
            field_dict.set("V", Object::string_literal(val_bytes));
            if let Some(sel) = selected {
                let idx = options.iter().position(|o| o == sel).unwrap() as i64;
                field_dict.set("I", Object::Array(vec![Object::Integer(idx)]));
            }
            if let Some(dv) = default_selected {
                field_dict.set(
                    "DV",
                    Object::string_literal(crate::appearance::encode_winansi(dv)),
                );
            }
            field_dict.set(
                "AP",
                Object::Dictionary(dictionary! {
                    "N" => Object::Reference(ap_id)
                }),
            );
            field_dict.set("P", Object::Reference(page_ids[*page]));

            apply_tooltip(&mut field_dict, tooltip);

            apply_mk_and_border(&mut field_dict, background, border);

            let field_id = doc.add_object(Object::Dictionary(field_dict));
            Ok(BuiltField {
                top_field_id: field_id,
                widgets: vec![(*page, field_id)],
            })
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
            let flags: i64 = (*read_only as i64) | ((*required as i64) << 1);

            let rect = Object::Array(vec![
                Object::Real(*x),
                Object::Real(*y),
                Object::Real(*x + *width),
                Object::Real(*y + *height),
            ]);

            let mut field_dict = Dictionary::new();
            field_dict.set("Type", Object::Name(b"Annot".to_vec()));
            field_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
            // Print flag (/F bit 3) so the field shows in printed output.
            field_dict.set("F", Object::Integer(4));
            field_dict.set("FT", Object::Name(b"Sig".to_vec()));
            field_dict.set("T", Object::string_literal(name.as_bytes().to_vec()));
            field_dict.set("Rect", rect);
            field_dict.set("Ff", Object::Integer(flags));
            field_dict.set("P", Object::Reference(page_ids[*page]));

            apply_tooltip(&mut field_dict, tooltip);

            apply_mk_and_border(&mut field_dict, background, border);

            let field_id = doc.add_object(Object::Dictionary(field_dict));
            Ok(BuiltField {
                top_field_id: field_id,
                widgets: vec![(*page, field_id)],
            })
        }
    }
}

/// Build the AcroForm and every field's widget annotation, appending widgets
/// to their pages' `/Annots`. Returns the AcroForm object id, or `None` when
/// no fields are defined.
fn build_fields_and_acroform(
    doc: &mut Document,
    fields: &[FieldDef],
    page_ids: &[ObjectId],
    embedded_fonts: &std::collections::HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &[FontDesc],
    fonts: &[u8],
) -> Result<Option<ObjectId>, String> {
    if fields.is_empty() {
        return Ok(None);
    }
    // Collect the distinct standard-14 fonts used by text/choice fields
    // (Helvetica is always present, since the form-level /DA references it).
    // Validate each up front so an unknown font fails before any object is
    // written. Register each font once in /DR/Font under its alias.
    let mut needed: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    needed.insert("Helvetica");
    for field in fields {
        let base = match field {
            FieldDef::Text { font, .. } | FieldDef::Choice { font, .. } => {
                font.as_deref().unwrap_or("Helvetica")
            }
            _ => continue,
        };
        // Reject anything `da_font_alias` doesn't map *and* anything whose
        // width table can't be resolved — the per-field appearance below
        // does `standard_14_widths(base).unwrap()`, so both must agree.
        if da_font_alias(base).is_none() || crate::appearance::standard_14_widths(base).is_none() {
            return Err(format!("unknown field font: {base}"));
        }
        needed.insert(base);
    }

    // alias -> font object id, and base-font -> (alias, object id). Helvetica
    // is added first so Helvetica-only forms stay byte-identical to before.
    let mut dr_fonts = Dictionary::new();
    let mut font_registry: std::collections::HashMap<&str, (&'static str, lopdf::ObjectId)> =
        std::collections::HashMap::new();
    let helv = doc.add_object(Object::Dictionary(font_dict("Helvetica")));
    dr_fonts.set("Helv", Object::Reference(helv));
    font_registry.insert("Helvetica", ("Helv", helv));
    for base in &needed {
        if *base == "Helvetica" {
            continue;
        }
        let alias = da_font_alias(base).unwrap();
        let fid = doc.add_object(Object::Dictionary(font_dict(base)));
        dr_fonts.set(alias, Object::Reference(fid));
        font_registry.insert(base, (alias, fid));
    }

    // Register every embedded font used by a text field under its `/BPF<n>`
    // alias, so the field's `/DA` and appearance can reference it and viewers
    // can resolve it via the AcroForm `/DR` without a per-appearance copy.
    // `embedded_aliases` keeps the computed `/BPF<n>` alias owned so
    // `resolve_create_font` can lend it to `FieldFont::Embedded` as a `&str`.
    let mut embedded_aliases: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();
    for field in fields {
        if let FieldDef::Text {
            font_id: Some(i), ..
        } = field
        {
            let alias = format!("BPF{i}");
            if !dr_fonts.has(alias.as_bytes()) {
                let (type0_id, _) = &embedded_fonts[i];
                dr_fonts.set(alias.as_bytes().to_vec(), Object::Reference(*type0_id));
            }
            embedded_aliases.insert(*i, alias);
        }
    }

    let mut acro_fields: Vec<Object> = Vec::new();
    // Track which field object ids go on which page: page_index -> Vec<ObjectId>
    let mut page_annots: Vec<Vec<lopdf::ObjectId>> = vec![Vec::new(); page_ids.len()];

    for field in fields {
        let font = resolve_create_font(
            field,
            &font_registry,
            embedded_fonts,
            &embedded_aliases,
            font_descs,
            fonts,
        );
        let built = build_one_field(doc, field, page_ids, font)?;
        acro_fields.push(Object::Reference(built.top_field_id));
        for (page_idx, widget_id) in built.widgets {
            page_annots[page_idx].push(widget_id);
        }
    }

    // Append widget annotations to their respective pages
    for (pg_idx, annot_ids) in page_annots.iter().enumerate() {
        if annot_ids.is_empty() {
            continue;
        }
        let page_obj = doc
            .get_object_mut(page_ids[pg_idx])
            .map_err(|e| format!("internal: page object {:?} missing: {e}", page_ids[pg_idx]))?;
        let page_dict = page_obj
            .as_dict_mut()
            .map_err(|e| format!("internal: page object is not a dict: {e}"))?;
        let annots = page_dict.get_mut(b"Annots").ok().and_then(|o| {
            if let Object::Array(_) = o {
                Some(o)
            } else {
                None
            }
        });
        if let Some(Object::Array(arr)) = annots {
            for &aid in annot_ids {
                arr.push(Object::Reference(aid));
            }
        } else {
            let arr: Vec<Object> = annot_ids
                .iter()
                .map(|&aid| Object::Reference(aid))
                .collect();
            page_dict.set("Annots", Object::Array(arr));
        }
    }

    // Build and add AcroForm
    let acro_dict = dictionary! {
        "Fields" => Object::Array(acro_fields),
        "DR" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dr_fonts)
        }),
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        "NeedAppearances" => Object::Boolean(false)
    };
    let acro_id = doc.add_object(Object::Dictionary(acro_dict));
    Ok(Some(acro_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    /// Walk page 0's Resources/XObject and return true if any entry resolves to
    /// a stream with /Subtype /Form whose key starts with `BPp`.
    fn page0_has_bpp_form_xobject(out: &[u8]) -> bool {
        let doc = Document::load_mem(out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = match page.get(b"Resources") {
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).unwrap(),
            Ok(Object::Dictionary(d)) => d,
            _ => return false,
        };
        let xo = match res.get(b"XObject") {
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).unwrap(),
            Ok(Object::Dictionary(d)) => d,
            _ => return false,
        };
        for (k, v) in xo.iter() {
            if !k.starts_with(b"BPp") {
                continue;
            }
            let id = match v {
                Object::Reference(r) => *r,
                _ => continue,
            };
            if let Ok(s) = doc.get_object(id).and_then(|o| o.as_stream())
                && s.dict.get(b"Subtype").and_then(|n| n.as_name()).ok() == Some(b"Form".as_ref())
            {
                return true;
            }
        }
        false
    }

    fn page0_content(out: &[u8]) -> String {
        let doc = Document::load_mem(out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let cid = match page.get(b"Contents").unwrap() {
            Object::Reference(r) => *r,
            Object::Array(a) => a[0].as_reference().unwrap(),
            _ => panic!("unexpected contents"),
        };
        let stream = doc.get_object(cid).unwrap().as_stream().unwrap();
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn creates_page_with_embedded_pdf_page() {
        let src = FICHA;
        let len = src.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"page","page":0,"x":10,"y":20,"width":300,"height":400,"imageOffset":0,"imageLength":{len},"srcPage":0}}]"#
        );
        let out = create_document_json(&json, src, &[], "[]", "[]").unwrap();
        assert!(
            page0_has_bpp_form_xobject(&out),
            "created page must carry a BPp Form XObject"
        );
        let content = page0_content(&out);
        assert!(
            content.contains("/BPp0 Do"),
            "content missing /BPp0 Do: {content}"
        );
    }

    fn tiny_png() -> &'static [u8] {
        // 1×1 RGBA PNG (color_type=6) — same bytes as tiny_rgba_png below
        &[
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
            0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
            0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }

    /// Explicit alias used by the new SMask tests.
    fn tiny_rgba_png() -> &'static [u8] {
        tiny_png()
    }

    /// 1×1 opaque RGB PNG (color_type=2) — no alpha channel.
    fn tiny_rgb_png() -> &'static [u8] {
        &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92,
            0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn builder_writes_default_values_for_all_field_types() {
        let ops = r#"[{"op":"addPage","width":595,"height":842}]"#;
        let fields = r#"[
            {"type":"text","name":"t","page":0,"x":10,"y":10,"width":100,"height":20,"defaultValue":"DEF"},
            {"type":"checkBox","name":"c","page":0,"x":10,"y":40,"size":12,"defaultChecked":true,"onValue":"Yes"},
            {"type":"radioGroup","name":"r","defaultSelected":"A","options":[
                {"value":"A","page":0,"x":10,"y":70,"size":12},
                {"value":"B","page":0,"x":40,"y":70,"size":12}
            ]},
            {"type":"choice","name":"d","page":0,"x":10,"y":100,"width":100,"height":20,"combo":true,"options":["X","Y"],"defaultSelected":"Y"}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let dv = |name: &str| {
            v.as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == name)
                .unwrap()["defaultValue"]
                .clone()
        };
        assert_eq!(dv("t"), "DEF");
        assert_eq!(dv("c"), "Yes");
        assert_eq!(dv("r"), "A");
        assert_eq!(dv("d"), "Y");
    }

    #[test]
    fn builder_text_reports_da_font_and_size() {
        let ops = r#"[{"op":"addPage","width":595,"height":842}]"#;
        let fields = r#"[
            {"type":"text","name":"amount","page":0,"x":10,"y":10,"width":100,"height":20,"fontSize":14},
            {"type":"checkBox","name":"agree","page":0,"x":10,"y":40,"size":12}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let by = |name: &str| {
            v.as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(by("amount")["fontName"], "Helv");
        assert_eq!(by("amount")["fontSize"], 14.0);
        // Non-text fields report null.
        assert_eq!(by("agree")["fontName"], serde_json::Value::Null);
        assert_eq!(by("agree")["fontSize"], serde_json::Value::Null);
    }

    #[test]
    fn builder_sets_print_flag_on_widgets() {
        let ops = r#"[{"op":"addPage","width":595,"height":842}]"#;
        let fields = r#"[
            {"type":"text","name":"t","page":0,"x":10,"y":10,"width":100,"height":20},
            {"type":"radioGroup","name":"r","options":[
                {"value":"A","page":0,"x":10,"y":40,"size":12},
                {"value":"B","page":0,"x":40,"y":40,"size":12}
            ]}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let by = |name: &str| {
            v.as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(by("t")["widgets"][0]["print"], true);
        for w in by("r")["widgets"].as_array().unwrap() {
            assert_eq!(w["print"], true);
        }
    }

    #[test]
    fn builder_sets_password_flag() {
        let ops = r#"[{"op":"addPage","width":595,"height":842}]"#;
        let fields = r#"[{"type":"text","name":"pin","page":0,"x":10,"y":10,"width":100,"height":20,"password":true}]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["name"], "pin");
        assert_eq!(v[0]["password"], true);
    }

    #[test]
    fn builder_rejects_default_selected_not_in_options() {
        let ops = r#"[{"op":"addPage","width":595,"height":842}]"#;
        let fields = r#"[{"type":"choice","name":"d","page":0,"x":10,"y":10,"width":100,"height":20,"combo":true,"options":["X","Y"],"defaultSelected":"Z"}]"#;
        let err = create_document_json(ops, &[], &[], "[]", fields).unwrap_err();
        assert!(err.contains("defaultSelected"), "got: {err}");
    }

    #[test]
    fn creates_single_page_doc() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            "[]",
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let cat = doc.catalog().unwrap();
        assert!(cat.has(b"Pages"));
    }

    #[test]
    fn page_has_mediabox() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            "[]",
        )
        .unwrap();
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
        let out = create_document_json(r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#, &[], &[], "[]", "[]").unwrap();
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
    fn creates_doc_with_embedded_font() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":50,"y":700,"size":24,"fontId":0,"color":[0,0,0],"text":"日本語"}]"#;
        let out = create_document_json(ops, &[], FONT, &fonts_json, "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
        let (_, fref) = fonts
            .iter()
            .find(|(k, _)| k.starts_with(b"BPE"))
            .expect("embedded font key");
        let f = doc
            .get_object(fref.as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(f.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
    }

    #[test]
    fn multiple_pages_in_order() {
        let out = create_document_json(r#"[{"op":"addPage","width":100,"height":200},{"op":"addPage","width":300,"height":400}]"#, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        assert_eq!(pages.len(), 2);
        let p0 = doc.get_dictionary(pages[0].1).unwrap();
        let mb0 = p0.get(b"MediaBox").unwrap().as_array().unwrap();
        assert!((mb0[2].as_float().unwrap() - 100.0).abs() < 0.5);
    }

    #[test]
    fn errors_on_no_pages() {
        let r = create_document_json(
            r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#,
            &[],
            &[],
            "[]",
            "[]",
        );
        assert!(r.is_err());
    }

    #[test]
    fn errors_on_text_page_out_of_range() {
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":1,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#,
            &[],
            &[],
            "[]",
            "[]",
        );
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842},{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#,
            &[],
            &[],
            "[]",
            "[]",
        );
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn output_parses_and_is_nonempty() {
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            "[]",
        )
        .unwrap();
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
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjs = res.get(b"XObject").unwrap().as_dict().unwrap();
        let bpi_entry = xobjs.iter().find(|(k, _)| k.starts_with(b"BPI"));
        assert!(
            bpi_entry.is_some(),
            "expected a BPI* key in XObject resources"
        );
        let contents_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(contents_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&stream.content);
        assert!(
            s.contains("/BPI0 Do"),
            "content stream should contain '/BPI0 Do', got: {s}"
        );
    }

    #[test]
    fn created_image_with_alpha_has_smask() {
        let png = tiny_rgba_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjs = res.get(b"XObject").unwrap().as_dict().unwrap();
        let bpi_entry = xobjs
            .iter()
            .find(|(k, _)| k.starts_with(b"BPI"))
            .expect("expected a BPI* key in XObject resources");
        let xobj_id = bpi_entry.1.as_reference().unwrap();
        let xobj_stream = doc.get_object(xobj_id).unwrap().as_stream().unwrap();
        let smask_val = xobj_stream
            .dict
            .get(b"SMask")
            .expect("alpha PNG image XObject should have /SMask");
        let smask_id = smask_val
            .as_reference()
            .expect("/SMask should be an indirect reference");
        let smask_stream = doc.get_object(smask_id).unwrap().as_stream().unwrap();
        assert_eq!(
            smask_stream
                .dict
                .get(b"ColorSpace")
                .unwrap()
                .as_name()
                .unwrap(),
            b"DeviceGray",
            "/SMask image should have DeviceGray color space"
        );
    }

    #[test]
    fn created_opaque_image_has_no_smask() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        let xobjs = res.get(b"XObject").unwrap().as_dict().unwrap();
        let bpi_entry = xobjs
            .iter()
            .find(|(k, _)| k.starts_with(b"BPI"))
            .expect("expected a BPI* key in XObject resources");
        let xobj_id = bpi_entry.1.as_reference().unwrap();
        let xobj_stream = doc.get_object(xobj_id).unwrap().as_stream().unwrap();
        assert!(
            xobj_stream.dict.get(b"SMask").is_err(),
            "opaque PNG image XObject should NOT have /SMask"
        );
    }

    #[test]
    fn image_rotate_emits_rotation_matrix() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len},"rotate":90}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let s = String::from_utf8_lossy(&out);
        // 90°: cos=0, sin=1 → "0 1 -1 0 0 0 cm"; plus the translate to (50,50).
        assert!(
            s.contains("0 1 -1 0 0 0 cm"),
            "expected 90° rotation matrix"
        );
        assert!(
            s.contains("1 0 0 1 50 50 cm"),
            "expected translate to placement point"
        );
    }

    #[test]
    fn image_no_transform_uses_combined_matrix() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains("100 0 0 80 50 50 cm"),
            "expected single combined cm when no rotate/skew"
        );
    }

    #[test]
    fn rectangle_dash_emits_dash_op() {
        let json = r#"[{"op":"addPage","width":595,"height":842},{"op":"rectangle","page":0,"x":10,"y":10,"width":100,"height":50,"borderColor":[0,0,0],"borderWidth":1,"dash":[4,2]}]"#;
        let out = create_document_json(json, &[], &[], "[]", "[]").unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("[4 2] 0 d"), "expected dash pattern op");
    }

    #[test]
    fn line_without_dash_has_no_dash_op() {
        let json = r#"[{"op":"addPage","width":595,"height":842},{"op":"line","page":0,"x1":0,"y1":0,"x2":50,"y2":50,"color":[0,0,0],"thickness":1}]"#;
        let out = create_document_json(json, &[], &[], "[]", "[]").unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(!s.contains(" d\n"), "solid line should emit no dash op");
    }

    #[test]
    fn created_image_opacity_registers_extgstate() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len},"opacity":0.5}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        // ExtGState resource exists with a /ca entry.
        let egs = res.get(b"ExtGState").unwrap().as_dict().unwrap();
        let (_, gs_ref) = egs.iter().next().expect("expected an ExtGState entry");
        let gs = doc
            .get_object(gs_ref.as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert!((gs.get(b"ca").unwrap().as_float().unwrap() - 0.5).abs() < 0.001);
        // Content stream references the gs and the image.
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let content = doc.get_object(content_id).unwrap().as_stream().unwrap();
        let s = String::from_utf8_lossy(&content.content);
        assert!(
            s.contains(" gs"),
            "image content should apply an ExtGState, got: {s}"
        );
    }

    #[test]
    fn created_image_without_opacity_has_no_extgstate() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = create_document_json(&json, png, &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = page.get(b"Resources").unwrap().as_dict().unwrap();
        // No ExtGState resource (or empty) when opacity omitted.
        if let Ok(egs) = res.get(b"ExtGState").and_then(|o| o.as_dict()) {
            assert!(egs.iter().next().is_none(), "expected no ExtGState entries");
        }
    }

    #[test]
    fn created_image_opacity_out_of_range_errors() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":0,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{len},"opacity":1.5}}]"#
        );
        let r = create_document_json(&json, png, &[], "[]", "[]");
        assert!(r.unwrap_err().contains("opacity"));
    }

    #[test]
    fn image_page_out_of_range_errors() {
        let png = tiny_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"addPage","width":595,"height":842}},{{"op":"image","page":1,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let r = create_document_json(&json, png, &[], "[]", "[]");
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
        let r = create_document_json(&json, png, &[], "[]", "[]");
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
            &[],
            "[]",
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
            &[],
            "[]",
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
            &[],
            "[]",
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
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            fields,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert!(doc.catalog().unwrap().has(b"AcroForm"));
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("fullName"), "json: {json}");
        assert!(json.contains("Ada"), "json: {json}");
    }

    #[test]
    fn text_field_on_page_annots() {
        let fields =
            r#"[{"type":"text","name":"a","page":0,"x":10,"y":10,"width":100,"height":20}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            fields,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(
            doc.get_dictionary(pid)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn rejects_duplicate_field_name() {
        let f = r#"[{"type":"text","name":"x","page":0,"x":0,"y":0,"width":10,"height":10},{"type":"text","name":"x","page":0,"x":0,"y":40,"width":10,"height":10}]"#;
        assert!(
            create_document_json(
                r#"[{"op":"addPage","width":595,"height":842}]"#,
                &[],
                &[],
                "[]",
                f
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_field_bad_page() {
        let f = r#"[{"type":"text","name":"x","page":5,"x":0,"y":0,"width":10,"height":10}]"#;
        assert!(
            create_document_json(
                r#"[{"op":"addPage","width":595,"height":842}]"#,
                &[],
                &[],
                "[]",
                f
            )
            .is_err()
        );
    }

    #[test]
    fn creates_checkbox_checked() {
        let f = r#"[{"type":"checkBox","name":"agree","page":0,"x":56,"y":660,"size":14,"checked":true}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("agree") && json.contains("\"type\":\"checkbox\""));
        assert!(json.contains("Yes"));
    }

    #[test]
    fn checkbox_custom_on_value() {
        let f = r#"[{"type":"checkBox","name":"c","page":0,"x":0,"y":0,"size":12,"onValue":"On"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        assert!(crate::forms::read_fields_json(&out).unwrap().contains("On"));
    }

    #[test]
    fn creates_radio_group() {
        let f = r#"[{"type":"radioGroup","name":"plan","selected":"pro","options":[{"value":"free","page":0,"x":56,"y":620,"size":14},{"value":"pro","page":0,"x":56,"y":600,"size":14}]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("\"type\":\"radio\""));
        assert!(json.contains("free") && json.contains("pro"));
        // parent in /Fields, 2 kids in page Annots
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(
            doc.get_dictionary(pid)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn radio_rejects_unknown_selected() {
        let f = r#"[{"type":"radioGroup","name":"p","selected":"nope","options":[{"value":"a","page":0,"x":0,"y":0,"size":12}]}]"#;
        assert!(
            create_document_json(
                r#"[{"op":"addPage","width":595,"height":842}]"#,
                &[],
                &[],
                "[]",
                f
            )
            .is_err()
        );
    }

    #[test]
    fn radio_rejects_empty_options() {
        let f = r#"[{"type":"radioGroup","name":"p","options":[]}]"#;
        assert!(
            create_document_json(
                r#"[{"op":"addPage","width":595,"height":842}]"#,
                &[],
                &[],
                "[]",
                f
            )
            .is_err()
        );
    }

    #[test]
    fn creates_dropdown() {
        let f = r#"[{"type":"choice","name":"country","page":0,"x":56,"y":560,"width":120,"height":20,"combo":true,"options":["AR","BR","CL"],"selected":"AR"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(json.contains("\"type\":\"dropdown\""), "json: {json}");
        assert!(
            json.contains("AR") && json.contains("BR") && json.contains("CL"),
            "json: {json}"
        );
    }

    #[test]
    fn creates_listbox() {
        let f = r#"[{"type":"choice","name":"langs","page":0,"x":56,"y":500,"width":120,"height":50,"combo":false,"options":["es","pt"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        assert!(
            crate::forms::read_fields_json(&out)
                .unwrap()
                .contains("\"type\":\"listbox\"")
        );
    }

    #[test]
    fn choice_rejects_unknown_selected() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"options":["a"],"selected":"z"}]"#;
        assert!(
            create_document_json(
                r#"[{"op":"addPage","width":595,"height":842}]"#,
                &[],
                &[],
                "[]",
                f
            )
            .is_err()
        );
    }

    #[test]
    fn creates_signature_field() {
        let f = r#"[{"type":"signature","name":"sig","page":0,"x":300,"y":560,"width":160,"height":60}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let json = crate::forms::read_fields_json(&out).unwrap();
        assert!(
            json.contains("\"type\":\"signature\"") && json.contains("sig"),
            "json: {json}"
        );
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(
            doc.get_dictionary(pid)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn creates_outline() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"addPage","width":595,"height":842},{"op":"outline","items":[{"title":"Cover","page":0},{"title":"Body","page":1,"children":[{"title":"Sub","page":1}]}]}]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        let outlines = doc
            .get_object(cat.get(b"Outlines").unwrap().as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(outlines.has(b"First") && outlines.has(b"Last"));
        assert!(outlines.get(b"Count").unwrap().as_i64().unwrap() >= 2);
    }

    #[test]
    fn outline_rejects_bad_page() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"outline","items":[{"title":"x","page":9}]}]"#;
        assert!(create_document_json(ops, &[], &[], "[]", "[]").is_err());
    }

    fn get_first_field_dict(doc: &Document) -> Dictionary {
        let cat = doc.catalog().unwrap();
        let acro = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("AcroForm is not a dict or ref"),
        };
        let fid = acro.get(b"Fields").unwrap().as_array().unwrap()[0]
            .as_reference()
            .unwrap();
        doc.get_dictionary(fid).unwrap().clone()
    }

    #[test]
    fn create_output_structurally_intact_after_refactor() {
        // A doc exercising a standard-14 text field, a checkbox, and a choice
        // field. Guards the Task 1 `build_one_field` extraction: create output
        // must stay structurally intact, backed by the full create suite.
        let ops = r#"[{"op":"addPage","width":300,"height":300}]"#;
        let fields = r#"[
            {"type":"text","name":"t","page":0,"x":10,"y":10,"width":100,"height":20},
            {"type":"checkBox","name":"c","page":0,"x":10,"y":40,"size":12},
            {"type":"choice","name":"d","page":0,"x":10,"y":70,"width":100,"height":20,"options":["a","b"],"combo":true}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", fields).unwrap();
        // Structural assertions (stable across environments):
        let doc = Document::load_mem(&out).unwrap();
        assert!(doc.catalog().unwrap().has(b"AcroForm"));
        let acro = get_first_field_dict(&doc); // existing test helper
        assert!(acro.has(b"T"));
    }

    #[test]
    fn field_border_and_background() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":10,"y":10,"width":100,"height":20,"border":{"color":[1,0,0],"width":2},"background":[0.9,0.9,0.9]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let mk = w.get(b"MK").unwrap().as_dict().unwrap();
        assert!(mk.has(b"BC"), "MK missing BC (border color)");
        assert!(mk.has(b"BG"), "MK missing BG (background)");
        let bs = w.get(b"BS").unwrap().as_dict().unwrap();
        assert!(
            (bs.get(b"W").unwrap().as_float().unwrap() - 2.0).abs() < 0.01,
            "BS/W should be 2.0"
        );
    }

    #[test]
    fn field_readonly_required_flags() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20,"readOnly":true,"required":true}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(ff & 1 != 0, "readOnly bit (bit 0) not set; Ff = {ff}");
        assert!(ff & 2 != 0, "required bit (bit 1) not set; Ff = {ff}");
    }

    #[test]
    fn field_tooltip() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20,"tooltip":"Your name"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        assert!(w.has(b"TU"), "field dict missing TU (tooltip)");
    }

    #[test]
    fn text_field_text_color_in_da() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20,"textColor":[1,0,0]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("1 0 0 rg"),
            "DA should contain red text color, got: {da}"
        );
    }

    #[test]
    fn choice_field_text_color_in_da() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"options":["a"],"textColor":[0,0,1]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("0 0 1 rg"),
            "DA should contain blue text color, got: {da}"
        );
    }

    #[test]
    fn text_field_align_and_font_size() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20,"align":"center","fontSize":18}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        assert_eq!(
            w.get(b"Q").unwrap().as_i64().unwrap(),
            1,
            "align center -> Q=1"
        );
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/Helv 18 Tf"),
            "DA should use font size 18, got: {da}"
        );
    }

    #[test]
    fn text_field_align_right_sets_q2() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20,"align":"right"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        assert_eq!(
            w.get(b"Q").unwrap().as_i64().unwrap(),
            2,
            "align right -> Q=2"
        );
    }

    #[test]
    fn text_field_default_no_q_and_size_12() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        assert!(!w.has(b"Q"), "no align -> Q should be absent");
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/Helv 12 Tf"),
            "default DA should use size 12, got: {da}"
        );
    }

    #[test]
    fn choice_field_align_and_font_size() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"options":["a"],"align":"right","fontSize":14}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        assert_eq!(
            w.get(b"Q").unwrap().as_i64().unwrap(),
            2,
            "align right -> Q=2"
        );
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/Helv 14 Tf"),
            "DA should use font size 14, got: {da}"
        );
    }

    #[test]
    fn comb_text_field_sets_flag_and_maxlen() {
        let f = r#"[{"type":"text","name":"ssn","page":0,"x":0,"y":0,"width":180,"height":24,"maxLength":9,"comb":true,"value":"12345"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(ff & (1 << 24) != 0, "Comb bit (24) not set; Ff = {ff}");
        assert_eq!(w.get(b"MaxLen").unwrap().as_i64().unwrap(), 9);
    }

    #[test]
    fn comb_field_without_maxlength_errors() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":180,"height":24,"comb":true}]"#;
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        );
        assert!(r.unwrap_err().contains("comb field requires maxLength"));
    }

    #[test]
    fn comb_field_multiline_errors() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":180,"height":24,"maxLength":9,"comb":true,"multiline":true}]"#;
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        );
        assert!(r.unwrap_err().contains("comb field cannot be multiline"));
    }

    #[test]
    fn comb_field_with_embedded_font_errors() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":180,"height":24,"maxLength":9,"comb":true,"fontId":0}]"#;
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            FONT,
            &fonts_json,
            f,
        );
        assert!(r.unwrap_err().contains("plain and multiline text fields only"));
    }

    #[test]
    fn comb_appearance_places_each_char() {
        // "AB" in a 5-cell comb should emit one absolute Tm per character.
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20,"maxLength":5,"comb":true,"value":"AB"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        let tm_count = s.matches(" Tm ").count();
        assert!(
            tm_count >= 2,
            "expected >=2 Tm ops (one per char), got {tm_count}"
        );
    }

    #[test]
    fn checkbox_square_style_draws_filled_rect() {
        let f = r#"[{"type":"checkBox","name":"c","page":0,"x":0,"y":0,"size":14,"checked":true,"checkStyle":"square"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains("re f"),
            "square style should emit a filled rectangle"
        );
    }

    #[test]
    fn checkbox_default_style_is_not_a_rect() {
        let f = r#"[{"type":"checkBox","name":"c","page":0,"x":0,"y":0,"size":14,"checked":true}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(
            !s.contains("re f"),
            "default checkbox should be a tick, not a filled rect"
        );
    }

    #[test]
    fn radio_check_style_square_draws_filled_rect() {
        let f = r#"[{"type":"radioGroup","name":"r","options":[{"value":"a","page":0,"x":0,"y":0,"size":12}],"checkStyle":"square"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains("re f"),
            "square radio style should emit a filled rectangle"
        );
    }

    #[test]
    fn radio_default_style_is_filled_circle() {
        // Filled circle uses bezier `c` ops ending in `f`, never a `re` rectangle.
        let f = r#"[{"type":"radioGroup","name":"r","options":[{"value":"a","page":0,"x":0,"y":0,"size":12}]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(
            !s.contains("re f"),
            "default radio should be a filled circle, not a rect"
        );
    }

    #[test]
    fn text_field_default_color_is_black() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":50,"height":20}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("0 g"),
            "default DA should be black '0 g', got: {da}"
        );
    }

    #[test]
    fn editable_combo_sets_edit_flag() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"editable":true,"options":["a","b"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(ff & (1 << 17) != 0, "Combo bit (17) not set; Ff = {ff}");
        assert!(ff & (1 << 18) != 0, "Edit bit (18) not set; Ff = {ff}");
    }

    #[test]
    fn non_editable_combo_has_no_edit_flag() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"options":["a","b"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(
            ff & (1 << 18) == 0,
            "Edit bit (18) must not be set without editable; Ff = {ff}"
        );
    }

    #[test]
    fn multiselect_listbox_sets_multiselect_flag() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":40,"combo":false,"multiselect":true,"options":["a","b"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(
            ff & (1 << 17) == 0,
            "Combo bit (18) must not be set on a list box; Ff = {ff}"
        );
        assert!(
            ff & (1 << 21) != 0,
            "Multiselect bit (22) not set; Ff = {ff}"
        );
        assert!(
            crate::forms::is_multiselect(ff),
            "is_multiselect should report true; Ff = {ff}"
        );
    }

    #[test]
    fn listbox_without_multiselect_has_no_multiselect_flag() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":40,"combo":false,"options":["a","b"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(
            ff & (1 << 21) == 0,
            "Multiselect bit (22) must not be set by default; Ff = {ff}"
        );
    }

    #[test]
    fn multiselect_ignored_on_combo_box() {
        // Combo boxes are never multi-select; the flag must be suppressed even if requested.
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":50,"height":20,"combo":true,"multiselect":true,"options":["a","b"]}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let ff = w.get(b"Ff").unwrap().as_i64().unwrap();
        assert!(
            ff & (1 << 21) == 0,
            "Multiselect bit (22) must not be set on a combo box; Ff = {ff}"
        );
    }

    #[test]
    fn created_doc_has_metadata() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"metadata","title":"Generated","author":"better-pdf"}]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let json = crate::metadata::read_metadata_json(&out).unwrap();
        assert!(json.contains("Generated"), "json was {json}");
        assert!(json.contains("better-pdf"), "json was {json}");
    }

    #[test]
    fn created_page_rotation_applied() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setRotation","page":0,"degrees":90}]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(
            doc.get_dictionary(pid)
                .unwrap()
                .get(b"Rotate")
                .unwrap()
                .as_i64()
                .unwrap(),
            90
        );
    }

    #[test]
    fn created_page_media_box_override() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setMediaBox","page":0,"box":[0,0,200,300]}]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let mb = doc
            .get_dictionary(pid)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        assert!((mb[2].as_float().unwrap() - 200.0).abs() < 0.5);
    }

    #[test]
    fn created_page_rotation_rejects_non_multiple() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"setRotation","page":0,"degrees":33}]"#;
        assert!(create_document_json(ops, &[], &[], "[]", "[]").is_err());
    }

    #[test]
    fn created_page_with_link_has_link_annot() {
        let ops = r#"[{"op":"addPage","width":595,"height":842},{"op":"link","page":0,"rect":[50,50,200,80],"uri":"https://example.com"}]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        let mut found = false;
        for a in annots {
            let d = doc
                .get_object(a.as_reference().unwrap())
                .unwrap()
                .as_dict()
                .unwrap();
            if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Link".as_ref()) {
                found = true;
                let act = d.get(b"A").unwrap().as_dict().unwrap();
                assert_eq!(act.get(b"S").unwrap().as_name().unwrap(), b"URI");
            }
        }
        assert!(found, "created page should have a /Link annot");
    }

    #[test]
    fn path_create() {
        let ops = r#"[
            {"op":"addPage","width":595,"height":842},
            {"op":"path","page":0,"segments":[
                {"t":"m","x":10,"y":10},
                {"t":"l","x":100,"y":10},
                {"t":"l","x":100,"y":100},
                {"t":"z"}
            ],"fill":[0,0,1],"stroke":[0,0,0],"strokeWidth":1.5}
        ]"#;
        let out = create_document_json(ops, &[], &[], "[]", "[]").unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let contents = doc.get_dictionary(pid).unwrap().get(b"Contents").unwrap();
        let content_id = match contents {
            lopdf::Object::Reference(r) => *r,
            _ => panic!("expected a reference"),
        };
        let stream = doc.get_object(content_id).unwrap().as_stream().unwrap();
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let s = String::from_utf8_lossy(&bytes).into_owned();
        assert!(s.contains("10 10 m"), "expected moveto: {s}");
        assert!(s.contains(" l"), "expected lineto: {s}");
        assert!(s.contains('B'), "expected fill+stroke paint op B: {s}");
    }

    #[test]
    fn da_font_alias_maps_all_standard_14() {
        assert_eq!(da_font_alias("Helvetica"), Some("Helv"));
        assert_eq!(da_font_alias("Times-Roman"), Some("TiRo"));
        assert_eq!(da_font_alias("Courier-Bold"), Some("CoBo"));
        assert_eq!(da_font_alias("Times-BoldItalic"), Some("TiBI"));
        assert_eq!(da_font_alias("Symbol"), None);
    }

    #[test]
    fn text_field_uses_requested_standard_14_font() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Times-Roman"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/TiRo"),
            "DA should reference the Times alias, got: {da}"
        );

        // /DR/Font has TiRo -> a font dict with BaseFont Times-Roman.
        let cat = doc.catalog().unwrap();
        let acro = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("AcroForm not dict/ref"),
        };
        let dr = acro.get(b"DR").unwrap().as_dict().unwrap();
        let fonts = dr.get(b"Font").unwrap().as_dict().unwrap();
        let tiro = match fonts.get(b"TiRo").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("TiRo not dict/ref"),
        };
        let base = tiro.get(b"BaseFont").unwrap().as_name().unwrap();
        assert_eq!(&String::from_utf8_lossy(base), "Times-Roman");
    }

    #[test]
    fn choice_field_uses_requested_font() {
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":80,"height":20,"combo":true,"options":["a","b"],"font":"Courier-Bold"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/CoBo"),
            "DA should reference the Courier-Bold alias, got: {da}"
        );
    }

    #[test]
    fn listbox_field_uses_requested_font() {
        // List boxes are choice fields with combo:false; the font path is shared
        // with dropdowns via _addChoice, but lock it in for regression safety.
        let f = r#"[{"type":"choice","name":"c","page":0,"x":0,"y":0,"width":80,"height":40,"combo":false,"options":["a","b"],"font":"Times-Bold"}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/TiBo"),
            "DA should reference the Times-Bold alias, got: {da}"
        );
    }

    #[test]
    fn default_font_is_helvetica_alias() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20}]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let w = get_first_field_dict(&doc);
        let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).to_string();
        assert!(
            da.contains("/Helv"),
            "default DA should use Helv, got: {da}"
        );
    }

    #[test]
    fn distinct_fonts_each_registered_once_in_dr() {
        let f = r#"[
            {"type":"text","name":"a","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Times-Roman"},
            {"type":"text","name":"b","page":0,"x":0,"y":40,"width":100,"height":20,"font":"Times-Roman"},
            {"type":"text","name":"c","page":0,"x":0,"y":80,"width":100,"height":20,"font":"Courier"}
        ]"#;
        let out = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        let acro = match cat.get(b"AcroForm").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            _ => panic!("AcroForm not dict/ref"),
        };
        let fonts = acro
            .get(b"DR")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(fonts.has(b"Helv"), "Helv always present");
        assert!(fonts.has(b"TiRo"), "TiRo present");
        assert!(fonts.has(b"Cour"), "Cour present");
        // TiRo used twice but registered once: exactly these three entries.
        assert_eq!(fonts.iter().count(), 3, "expected exactly Helv/TiRo/Cour");
    }

    #[test]
    fn unknown_field_font_is_rejected() {
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":100,"height":20,"font":"Comic Sans"}]"#;
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            &[],
            "[]",
            f,
        );
        assert!(r.is_err(), "unknown font must be rejected");
    }

    #[test]
    fn out_of_range_font_id_errors() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
        let f = r#"[{"type":"text","name":"t","page":0,"x":0,"y":0,"width":180,"height":24,"fontId":5}]"#;
        let r = create_document_json(
            r#"[{"op":"addPage","width":595,"height":842}]"#,
            &[],
            FONT,
            &fonts_json,
            f,
        );
        assert!(r.unwrap_err().contains("out of range"));
    }
}
