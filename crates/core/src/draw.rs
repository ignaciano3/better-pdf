//! Draw engine: apply draw ops (text, images, etc.) to existing PDF pages via
//! incremental update.

use lopdf::{Dictionary, IncrementalDocument, Object, ObjectId, Stream, dictionary};
use serde::Deserialize;

use crate::appearance::{encode_winansi, escape_pdf_literal};
use crate::fonts::{BuiltFont, EmbeddedFontInput, build_embedded_font};
use std::collections::{BTreeSet, HashMap};
use std::io::Write;

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
#[serde(tag = "op", rename_all = "lowercase")]
pub(crate) enum DrawOp {
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
    #[serde(rename = "page")]
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
    #[serde(rename = "setRotation")]
    SetRotation { page: usize, degrees: i64 },
    #[serde(rename = "link")]
    Link {
        page: usize,
        rect: [f32; 4],
        uri: Option<String>,
        #[serde(rename = "goToPage")]
        go_to_page: Option<usize>,
    },
    #[serde(rename = "setMediaBox")]
    SetMediaBox {
        page: usize,
        #[serde(rename = "box")]
        media_box: [f32; 4],
    },
    #[serde(rename = "path")]
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
}

/// A single path segment, tagged by `t`.
#[derive(Deserialize, Clone)]
#[serde(tag = "t", rename_all = "lowercase")]
pub(crate) enum Seg {
    M {
        x: f32,
        y: f32,
    },
    L {
        x: f32,
        y: f32,
    },
    C {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    Z,
}

pub(crate) const STANDARD_14: &[&str] = &[
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
pub(crate) fn fmt_num(v: f32) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    if (rounded - rounded.floor()).abs() < 0.001 {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{:.2}", rounded);
        let s = s.trim_end_matches('0');
        s.to_string()
    }
}

pub(crate) fn standard_14_index(font: &str) -> Option<usize> {
    STANDARD_14.iter().position(|&f| f == font)
}

pub(crate) fn font_dict(base_font: &str) -> lopdf::Dictionary {
    dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }
}

/// Build a `/Annot /Subtype /Link` dictionary. Exactly one of `uri` /
/// `dest_page` should be Some (validated by the caller). `/Border [0 0 0]`
/// suppresses the visible link box. For an internal jump, `/Dest` is
/// `[Reference(dest_page) /XYZ null null null]` (keep current zoom/position).
pub(crate) fn link_annot_dict(
    rect: [f32; 4],
    uri: Option<&str>,
    dest_page: Option<ObjectId>,
) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(b"Link".to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[2]),
            Object::Real(rect[3]),
        ]),
    );
    d.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );
    if let Some(uri) = uri {
        let mut action = Dictionary::new();
        action.set("S", Object::Name(b"URI".to_vec()));
        action.set("URI", Object::string_literal(uri.as_bytes().to_vec()));
        d.set("A", Object::Dictionary(action));
    } else if let Some(dest) = dest_page {
        d.set(
            "Dest",
            Object::Array(vec![
                Object::Reference(dest),
                Object::Name(b"XYZ".to_vec()),
                Object::Null,
                Object::Null,
                Object::Null,
            ]),
        );
    }
    d
}

/// Append one self-contained `BT … ET` text block to `out`. `BT` resets the
/// text matrix to identity, so `(x, y)` is an absolute page position.
/// `font_key` is the resource name without leading slash, e.g. "BPF0".
/// If `rotate` is Some(degrees), wraps the block in `q`/`Q` and emits a `cm`
/// matrix before `BT`; `gs_key` optionally applies an ExtGState for opacity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_text_block(
    out: &mut Vec<u8>,
    font_key: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 3],
    text: &str,
    line_height: Option<f32>,
    rotate: Option<f32>,
    gs_key: Option<&str>,
) {
    let leading = line_height.unwrap_or(size * 1.15);
    let [r, g, b] = color;
    let wrap = rotate.is_some() || gs_key.is_some();
    if wrap {
        out.extend_from_slice(b"q\n");
    }
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    if let Some(deg) = rotate {
        let t = deg.to_radians();
        let (sin_, cos_) = (t.sin(), t.cos());
        writeln!(
            out,
            "{} {} {} {} {} {} cm",
            fmt_num(cos_),
            fmt_num(sin_),
            fmt_num(-sin_),
            fmt_num(cos_),
            fmt_num(x),
            fmt_num(y)
        )
        .unwrap();
    }
    out.extend_from_slice(b"BT\n");
    writeln!(out, "/{font_key} {} Tf", fmt_num(size)).unwrap();
    writeln!(out, "{} {} {} rg", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    writeln!(out, "{} TL", fmt_num(leading)).unwrap();
    if rotate.is_some() {
        out.extend_from_slice(b"0 0 Td\n");
    } else {
        writeln!(out, "{} {} Td", fmt_num(x), fmt_num(y)).unwrap();
    }
    for (i, line) in text.split('\n').enumerate() {
        let escaped = escape_pdf_literal(&encode_winansi(line));
        let escaped_str = String::from_utf8_lossy(&escaped).into_owned();
        if i == 0 {
            writeln!(out, "({escaped_str}) Tj").unwrap();
        } else {
            write!(out, "T*\n({escaped_str}) Tj\n").unwrap();
        }
    }
    out.extend_from_slice(b"ET\n");
    if wrap {
        out.extend_from_slice(b"Q\n");
    }
}

/// Like `emit_text_block`, but for a Type0/Identity-H font: each line is a list
/// of 2-byte glyph ids, emitted as a hex string `<....>`.
/// If `rotate` is Some(degrees), wraps the block in `q`/`Q` and emits a `cm`
/// matrix before `BT`; `gs_key` optionally applies an ExtGState for opacity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_text_block_cid(
    out: &mut Vec<u8>,
    font_key: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [f32; 3],
    gids_per_line: &[Vec<u16>],
    line_height: Option<f32>,
    rotate: Option<f32>,
    gs_key: Option<&str>,
) {
    let leading = line_height.unwrap_or(size * 1.15);
    let [r, g, b] = color;
    let wrap = rotate.is_some() || gs_key.is_some();
    if wrap {
        out.extend_from_slice(b"q\n");
    }
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    if let Some(deg) = rotate {
        let t = deg.to_radians();
        let (sin_, cos_) = (t.sin(), t.cos());
        writeln!(
            out,
            "{} {} {} {} {} {} cm",
            fmt_num(cos_),
            fmt_num(sin_),
            fmt_num(-sin_),
            fmt_num(cos_),
            fmt_num(x),
            fmt_num(y)
        )
        .unwrap();
    }
    out.extend_from_slice(b"BT\n");
    writeln!(out, "/{font_key} {} Tf", fmt_num(size)).unwrap();
    writeln!(out, "{} {} {} rg", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    writeln!(out, "{} TL", fmt_num(leading)).unwrap();
    if rotate.is_some() {
        out.extend_from_slice(b"0 0 Td\n");
    } else {
        writeln!(out, "{} {} Td", fmt_num(x), fmt_num(y)).unwrap();
    }
    for (i, line) in gids_per_line.iter().enumerate() {
        let mut hex = String::with_capacity(line.len() * 4);
        for gid in line {
            hex.push_str(&format!("{gid:04X}"));
        }
        if i == 0 {
            writeln!(out, "<{hex}> Tj").unwrap();
        } else {
            write!(out, "T*\n<{hex}> Tj\n").unwrap();
        }
    }
    out.extend_from_slice(b"ET\n");
    if wrap {
        out.extend_from_slice(b"Q\n");
    }
}

/// Append the CTM (`cm`) operators that place a unit XObject at `(x, y)` scaled
/// by `(sx, sy)`, optionally rotated by `rotate` degrees (counter-clockwise) and
/// skewed by `x_skew`/`y_skew` degrees. Matches pdf-lib's order:
/// translate → rotate → scale → skew. When there is no rotation or skew this
/// collapses to the single combined `sx 0 0 sy x y cm` form.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_placement(
    out: &mut Vec<u8>,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    rotate: f32,
    x_skew: f32,
    y_skew: f32,
) {
    if rotate == 0.0 && x_skew == 0.0 && y_skew == 0.0 {
        writeln!(
            out,
            "{} 0 0 {} {} {} cm",
            fmt_num(sx),
            fmt_num(sy),
            fmt_num(x),
            fmt_num(y)
        )
        .unwrap();
        return;
    }
    // translate to the placement point
    writeln!(out, "1 0 0 1 {} {} cm", fmt_num(x), fmt_num(y)).unwrap();
    // rotate about that point
    if rotate != 0.0 {
        let r = rotate.to_radians();
        writeln!(
            out,
            "{} {} {} {} 0 0 cm",
            fmt_num(r.cos()),
            fmt_num(r.sin()),
            fmt_num(-r.sin()),
            fmt_num(r.cos())
        )
        .unwrap();
    }
    // scale to the target box
    writeln!(out, "{} 0 0 {} 0 0 cm", fmt_num(sx), fmt_num(sy)).unwrap();
    // skew (pdf-lib: [1, tan(yskew), tan(xskew), 1])
    if x_skew != 0.0 || y_skew != 0.0 {
        writeln!(
            out,
            "1 {} {} 1 0 0 cm",
            fmt_num(y_skew.to_radians().tan()),
            fmt_num(x_skew.to_radians().tan())
        )
        .unwrap();
    }
}

/// Append a `q … cm /key Do Q` image-draw block. `(x,y)` is lower-left; width/height in points.
/// `gs_key` optionally applies an ExtGState (for opacity) before the image is painted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_image_op(
    out: &mut Vec<u8>,
    xobj_key: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gs_key: Option<&str>,
    rotate: f32,
    x_skew: f32,
    y_skew: f32,
) {
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    emit_placement(out, x, y, width, height, rotate, x_skew, y_skew);
    writeln!(out, "/{xobj_key} Do").unwrap();
    out.extend_from_slice(b"Q\n");
}

/// Append a dash-pattern operator (`[a b ...] phase d`) when `dash` is
/// non-empty. An empty `dash` leaves the stroke solid (no operator emitted).
fn emit_dash(out: &mut Vec<u8>, dash: &[f32], phase: f32) {
    if dash.is_empty() {
        return;
    }
    out.extend_from_slice(b"[");
    for (i, v) in dash.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b" ");
        }
        out.extend_from_slice(fmt_num(*v).as_bytes());
    }
    writeln!(out, "] {} d", fmt_num(phase)).unwrap();
}

fn paint_op(has_fill: bool, has_stroke: bool) -> &'static str {
    match (has_fill, has_stroke) {
        (true, true) => "B",
        (true, false) => "f",
        (false, true) => "S",
        (false, false) => "n",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_line(
    out: &mut Vec<u8>,
    gs_key: Option<&str>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: [f32; 3],
    dash: &[f32],
    dash_phase: f32,
) {
    let [r, g, b] = color;
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    writeln!(out, "{} w", fmt_num(thickness)).unwrap();
    emit_dash(out, dash, dash_phase);
    writeln!(out, "{} {} {} RG", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    writeln!(out, "{} {} m", fmt_num(x1), fmt_num(y1)).unwrap();
    writeln!(out, "{} {} l", fmt_num(x2), fmt_num(y2)).unwrap();
    out.extend_from_slice(b"S\nQ\n");
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_rectangle(
    out: &mut Vec<u8>,
    gs_key: Option<&str>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fill: Option<[f32; 3]>,
    border: Option<[f32; 3]>,
    border_width: Option<f32>,
    dash: &[f32],
    dash_phase: f32,
) {
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    if let Some([r, g, b]) = fill {
        writeln!(out, "{} {} {} rg", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    }
    if let Some([r, g, b]) = border {
        writeln!(out, "{} {} {} RG", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
        writeln!(out, "{} w", fmt_num(border_width.unwrap_or(1.0))).unwrap();
        emit_dash(out, dash, dash_phase);
    }
    writeln!(
        out,
        "{} {} {} {} re",
        fmt_num(x),
        fmt_num(y),
        fmt_num(w),
        fmt_num(h)
    )
    .unwrap();
    out.extend_from_slice(paint_op(fill.is_some(), border.is_some()).as_bytes());
    out.extend_from_slice(b"\nQ\n");
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_ellipse(
    out: &mut Vec<u8>,
    gs_key: Option<&str>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    fill: Option<[f32; 3]>,
    border: Option<[f32; 3]>,
    border_width: Option<f32>,
    dash: &[f32],
    dash_phase: f32,
) {
    // 4-segment cubic Bézier approximation. k = 4/3*(sqrt(2)-1) ≈ 0.5523.
    let k = 0.552_284_8_f32;
    let (ox, oy) = (rx * k, ry * k);
    out.extend_from_slice(b"q\n");
    if let Some(key) = gs_key {
        writeln!(out, "/{key} gs").unwrap();
    }
    if let Some([r, g, b]) = fill {
        writeln!(out, "{} {} {} rg", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    }
    if let Some([r, g, b]) = border {
        writeln!(out, "{} {} {} RG", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
        writeln!(out, "{} w", fmt_num(border_width.unwrap_or(1.0))).unwrap();
        emit_dash(out, dash, dash_phase);
    }
    // Start at right vertex, go counter-clockwise.
    writeln!(out, "{} {} m", fmt_num(cx + rx), fmt_num(cy)).unwrap();
    writeln!(
        out,
        "{} {} {} {} {} {} c",
        fmt_num(cx + rx),
        fmt_num(cy + oy),
        fmt_num(cx + ox),
        fmt_num(cy + ry),
        fmt_num(cx),
        fmt_num(cy + ry)
    )
    .unwrap();
    writeln!(
        out,
        "{} {} {} {} {} {} c",
        fmt_num(cx - ox),
        fmt_num(cy + ry),
        fmt_num(cx - rx),
        fmt_num(cy + oy),
        fmt_num(cx - rx),
        fmt_num(cy)
    )
    .unwrap();
    writeln!(
        out,
        "{} {} {} {} {} {} c",
        fmt_num(cx - rx),
        fmt_num(cy - oy),
        fmt_num(cx - ox),
        fmt_num(cy - ry),
        fmt_num(cx),
        fmt_num(cy - ry)
    )
    .unwrap();
    writeln!(
        out,
        "{} {} {} {} {} {} c",
        fmt_num(cx + ox),
        fmt_num(cy - ry),
        fmt_num(cx + rx),
        fmt_num(cy - oy),
        fmt_num(cx + rx),
        fmt_num(cy)
    )
    .unwrap();
    out.extend_from_slice(b"h\n");
    out.extend_from_slice(paint_op(fill.is_some(), border.is_some()).as_bytes());
    out.extend_from_slice(b"\nQ\n");
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_path(
    out: &mut Vec<u8>,
    gs_key: Option<&str>,
    segments: &[Seg],
    fill: Option<[f32; 3]>,
    stroke: Option<[f32; 3]>,
    stroke_width: Option<f32>,
    dash: &[f32],
    dash_phase: f32,
) {
    out.extend_from_slice(b"q\n");
    if let Some(k) = gs_key {
        writeln!(out, "/{k} gs").unwrap();
    }
    if let Some([r, g, b]) = fill {
        writeln!(out, "{} {} {} rg", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
    }
    if let Some([r, g, b]) = stroke {
        writeln!(out, "{} {} {} RG", fmt_num(r), fmt_num(g), fmt_num(b)).unwrap();
        writeln!(out, "{} w", fmt_num(stroke_width.unwrap_or(1.0))).unwrap();
        emit_dash(out, dash, dash_phase);
    }
    for seg in segments {
        match seg {
            Seg::M { x, y } => {
                writeln!(out, "{} {} m", fmt_num(*x), fmt_num(*y)).unwrap();
            }
            Seg::L { x, y } => {
                writeln!(out, "{} {} l", fmt_num(*x), fmt_num(*y)).unwrap();
            }
            Seg::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                writeln!(
                    out,
                    "{} {} {} {} {} {} c",
                    fmt_num(*x1),
                    fmt_num(*y1),
                    fmt_num(*x2),
                    fmt_num(*y2),
                    fmt_num(*x),
                    fmt_num(*y)
                )
                .unwrap();
            }
            Seg::Z => {
                out.extend_from_slice(b"h\n");
            }
        }
    }
    out.extend_from_slice(paint_op(fill.is_some(), stroke.is_some()).as_bytes());
    out.extend_from_slice(b"\nQ\n");
}

pub(crate) fn extgstate_dict(opacity: f32) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"ExtGState".to_vec()));
    d.set("ca", Object::Real(opacity)); // fill alpha
    d.set("CA", Object::Real(opacity)); // stroke alpha
    d
}

/// Validate that a 0-based page index is within `page_count`.
pub(crate) fn check_page(page: usize, page_count: usize) -> Result<(), String> {
    if page >= page_count {
        return Err(format!("page {page} out of range ({page_count} pages)"));
    }
    Ok(())
}

/// Validate an optional opacity is finite and in `0..=1`.
pub(crate) fn check_opacity(opacity: &Option<f32>) -> Result<(), String> {
    if let Some(o) = opacity
        && (!o.is_finite() || *o < 0.0 || *o > 1.0)
    {
        return Err("opacity must be in 0..1".to_string());
    }
    Ok(())
}

/// Validate every value in `values` is finite, returning `err` otherwise.
pub(crate) fn check_finite(values: &[f32], err: &str) -> Result<(), String> {
    for &v in values {
        if !v.is_finite() {
            return Err(err.to_string());
        }
    }
    Ok(())
}

/// Validate an optional RGB color's components are all finite.
pub(crate) fn check_color(color: &Option<[f32; 3]>) -> Result<(), String> {
    match color {
        Some(c) => check_finite(c, "invalid color"),
        None => Ok(()),
    }
}

/// Register an opacity `/ExtGState` for `opacity` (if any), returning its page
/// resource key (`BPGn`). `gs_counter` keeps keys unique across all pages.
fn alloc_opacity_gs(
    opacity: &Option<f32>,
    gs_counter: &mut usize,
    extgstates_on_page: &mut Vec<(String, ObjectId)>,
    inc: &mut IncrementalDocument,
) -> Option<String> {
    let o = (*opacity)?;
    let key = format!("BPG{gs_counter}");
    *gs_counter += 1;
    let gs_id = inc
        .new_document
        .add_object(Object::Dictionary(extgstate_dict(o)));
    extgstates_on_page.push((key.clone(), gs_id));
    Some(key)
}

pub(crate) fn register_extgstate(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    key: &str,
    gs_id: ObjectId,
) -> Result<(), String> {
    let res_ref = match dict_mut(inc, page_id)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match res_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            resolve_and_set_subdict(inc, id, b"ExtGState", key, gs_id)?;
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            if !page.has(b"Resources") {
                page.set("Resources", Object::Dictionary(Dictionary::new()));
            }
            let extgstate_sub_ref = page
                .get_mut(b"Resources")
                .and_then(Object::as_dict_mut)
                .ok()
                .and_then(|res| res.get(b"ExtGState").ok().cloned())
                .and_then(|obj| {
                    if let Object::Reference(id) = obj {
                        Some(id)
                    } else {
                        None
                    }
                });
            if let Some(sub_id) = extgstate_sub_ref {
                inc.opt_clone_object_to_new_document(sub_id)
                    .map_err(|e| e.to_string())?;
                let gs_dict = inc
                    .new_document
                    .get_object_mut(sub_id)
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                gs_dict.set(key.as_bytes().to_vec(), Object::Reference(gs_id));
            } else {
                let res = dict_mut(inc, page_id)?
                    .get_mut(b"Resources")
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                set_extgstate(res, key, gs_id);
            }
        }
    }
    Ok(())
}

fn set_extgstate(res: &mut Dictionary, key: &str, gs_id: ObjectId) {
    if !res.has(b"ExtGState") {
        res.set("ExtGState", Object::Dictionary(Dictionary::new()));
    }
    if let Ok(gs) = res.get_mut(b"ExtGState").and_then(Object::as_dict_mut) {
        gs.set(key.as_bytes().to_vec(), Object::Reference(gs_id));
    }
}

/// Register key -> xobject_id under the page's /Resources/XObject. Mirrors register_font.
pub(crate) fn register_xobject(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    key: &str,
    xobject_id: ObjectId,
) -> Result<(), String> {
    let res_ref = match dict_mut(inc, page_id)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match res_ref {
        Some(id) => {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            resolve_and_set_subdict(inc, id, b"XObject", key, xobject_id)?;
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            if !page.has(b"Resources") {
                page.set("Resources", Object::Dictionary(Dictionary::new()));
            }
            let xobject_sub_ref = page
                .get_mut(b"Resources")
                .and_then(Object::as_dict_mut)
                .ok()
                .and_then(|res| res.get(b"XObject").ok().cloned())
                .and_then(|obj| {
                    if let Object::Reference(id) = obj {
                        Some(id)
                    } else {
                        None
                    }
                });
            if let Some(sub_id) = xobject_sub_ref {
                inc.opt_clone_object_to_new_document(sub_id)
                    .map_err(|e| e.to_string())?;
                let xo_dict = inc
                    .new_document
                    .get_object_mut(sub_id)
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                xo_dict.set(key.as_bytes().to_vec(), Object::Reference(xobject_id));
            } else {
                let res = dict_mut(inc, page_id)?
                    .get_mut(b"Resources")
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                set_xobject(res, key, xobject_id);
            }
        }
    }
    Ok(())
}

fn set_xobject(res: &mut Dictionary, key: &str, xobject_id: ObjectId) {
    if !res.has(b"XObject") {
        res.set("XObject", Object::Dictionary(Dictionary::new()));
    }
    if let Ok(xo) = res.get_mut(b"XObject").and_then(Object::as_dict_mut) {
        xo.set(key.as_bytes().to_vec(), Object::Reference(xobject_id));
    }
}

/// If the sub-dict under `res_id[subdict_key]` is an indirect reference, clone it into
/// `new_document` and mutate it there; otherwise fall through to the inline set_* helper.
/// This prevents a silent no-op when the sub-dict is stored as an indirect object.
fn resolve_and_set_subdict(
    inc: &mut IncrementalDocument,
    res_id: ObjectId,
    subdict_key: &[u8],
    entry_key: &str,
    entry_id: ObjectId,
) -> Result<(), String> {
    // Check if the sub-dict is an indirect reference in either document
    let subdict_ref: Option<ObjectId> = inc
        .new_document
        .get_object(res_id)
        .and_then(Object::as_dict)
        .ok()
        .and_then(|d| d.get(subdict_key).ok().cloned())
        .and_then(|obj| {
            if let Object::Reference(id) = obj {
                Some(id)
            } else {
                None
            }
        });

    match subdict_ref {
        Some(sub_id) => {
            // Clone the indirect sub-dict into new_document so we can mutate it
            inc.opt_clone_object_to_new_document(sub_id)
                .map_err(|e| e.to_string())?;
            let sub_dict = inc
                .new_document
                .get_object_mut(sub_id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            sub_dict.set(entry_key.as_bytes().to_vec(), Object::Reference(entry_id));
        }
        None => {
            // Sub-dict is inline (or absent) — use the regular mutable setter
            let res = dict_mut(inc, res_id)?;
            // Route to the appropriate set_* based on subdict_key
            match subdict_key {
                b"ExtGState" => set_extgstate(res, entry_key, entry_id),
                b"XObject" => set_xobject(res, entry_key, entry_id),
                b"Font" => set_font(res, entry_key, entry_id),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Apply draw ops from a JSON string to `data` and return new PDF bytes
/// (incremental save). `images` is the concatenated image blob that Image ops
/// index into via imageOffset / imageLength.
pub fn apply_draw_ops_json(
    data: &[u8],
    ops_json: &str,
    images: &[u8],
    fonts: &[u8],
    fonts_json: &str,
    compress: bool,
) -> Result<Vec<u8>, String> {
    let ops: Vec<DrawOp> =
        serde_json::from_str(ops_json).map_err(|e| format!("invalid draw ops: {e}"))?;
    let font_descs: Vec<FontDesc> =
        serde_json::from_str(fonts_json).map_err(|e| format!("invalid fonts: {e}"))?;

    let doc = crate::doc_io::load_pdf(data)?;
    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    validate_draw_ops(&inc, &ops, images, fonts, &font_descs)?;
    let used = draw_used_chars(&ops);
    let mut add = |o: Object| inc.new_document.add_object(o);
    let built = build_document_fonts(&mut add, &font_descs, fonts, &used)?;
    draw_apply(&mut inc, &ops, images, fonts, &font_descs, &built)?;

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Validate draw ops and apply them to the incremental document: font-embedding
/// pre-pass followed by per-page content emission. Reads the base page list from
/// `inc.get_prev_documents()`, so it composes correctly after other mutators
/// (fill/flatten) on the same `inc`.
pub(crate) fn draw_apply(
    inc: &mut IncrementalDocument,
    ops: &[DrawOp],
    images: &[u8],
    fonts: &[u8],
    font_descs: &[FontDesc],
    built: &HashMap<usize, (ObjectId, BuiltFont)>,
) -> Result<(), String> {
    validate_draw_ops(inc, ops, images, fonts, font_descs)?;

    // Group ops by page index (preserving op order within each page)
    let mut page_ops: Vec<(usize, Vec<&DrawOp>)> = Vec::new();
    for op in ops {
        let page_idx = match op {
            DrawOp::Text { page, .. } => *page,
            DrawOp::Image { page, .. } => *page,
            DrawOp::Line { page, .. } => *page,
            DrawOp::Rectangle { page, .. } => *page,
            DrawOp::Ellipse { page, .. } => *page,
            DrawOp::Page { page, .. } => *page,
            DrawOp::SetRotation { page, .. } => *page,
            DrawOp::SetMediaBox { page, .. } => *page,
            DrawOp::Link { page, .. } => *page,
            DrawOp::Path { page, .. } => *page,
        };
        if let Some(entry) = page_ops.iter_mut().find(|(idx, _)| *idx == page_idx) {
            entry.1.push(op);
        } else {
            page_ops.push((page_idx, vec![op]));
        }
    }

    emit_page_ops(inc, &page_ops, built, font_descs, fonts, images)?;
    Ok(())
}

/// Validate every draw op (and font byte ranges) against the base page count
/// before any mutation, so an invalid request fails without touching `inc`.
fn validate_draw_ops(
    inc: &IncrementalDocument,
    ops: &[DrawOp],
    images: &[u8],
    fonts: &[u8],
    font_descs: &[FontDesc],
) -> Result<(), String> {
    let page_count = inc.get_prev_documents().get_pages().len();

    // Validate font descriptor byte ranges up front.
    for fd in font_descs {
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        if end > fonts.len() {
            return Err("font range out of bounds".to_string());
        }
    }

    // Validate ALL ops before mutating anything
    for op in ops {
        match op {
            DrawOp::Text {
                page,
                font,
                font_id,
                opacity,
                rotate,
                max_width,
                ..
            } => {
                check_page(*page, page_count)?;
                if let Some(i) = font_id {
                    if *i >= font_descs.len() {
                        return Err(format!("font id {i} out of range"));
                    }
                } else if !STANDARD_14.contains(&font.as_str()) {
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
            DrawOp::Image {
                page,
                image_offset,
                image_length,
                opacity,
                ..
            } => {
                check_page(*page, page_count)?;
                check_opacity(opacity)?;
                let end = image_offset
                    .checked_add(*image_length)
                    .ok_or_else(|| "image range out of bounds".to_string())?;
                if end > images.len() {
                    return Err("image range out of bounds".to_string());
                }
                // Validate that the image bytes are decodable
                crate::appearance::signature_image(&images[*image_offset..end])?;
            }
            DrawOp::Page {
                page,
                width,
                height,
                image_offset,
                image_length,
                opacity,
                ..
            } => {
                check_page(*page, page_count)?;
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
            DrawOp::Line {
                page,
                thickness,
                color,
                opacity,
                x1,
                y1,
                x2,
                y2,
                ..
            } => {
                check_page(*page, page_count)?;
                check_opacity(opacity)?;
                if let Some(t) = thickness
                    && (!t.is_finite() || *t < 0.0)
                {
                    return Err("thickness must be >= 0".to_string());
                }
                check_finite(&[*x1, *y1, *x2, *y2], "invalid coordinate")?;
                check_color(color)?;
            }
            DrawOp::Rectangle {
                page,
                color,
                border_color,
                border_width,
                opacity,
                x,
                y,
                width,
                height,
                ..
            } => {
                check_page(*page, page_count)?;
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
            DrawOp::Ellipse {
                page,
                color,
                border_color,
                border_width,
                opacity,
                x,
                y,
                x_scale,
                y_scale,
                ..
            } => {
                check_page(*page, page_count)?;
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
            DrawOp::SetRotation { page, degrees } => {
                check_page(*page, page_count)?;
                if degrees.rem_euclid(90) != 0 {
                    return Err("rotation degrees must be a multiple of 90".to_string());
                }
            }
            DrawOp::SetMediaBox { page, media_box } => {
                check_page(*page, page_count)?;
                check_finite(media_box, "invalid media box")?;
                if media_box[2] <= media_box[0] || media_box[3] <= media_box[1] {
                    return Err("invalid media box".to_string());
                }
            }
            DrawOp::Link {
                page,
                rect,
                uri,
                go_to_page,
            } => {
                check_page(*page, page_count)?;
                match (uri.is_some(), go_to_page.is_some()) {
                    (true, true) => {
                        return Err("link must have exactly one of uri or goToPage".to_string());
                    }
                    (false, false) => {
                        return Err("link must have exactly one of uri or goToPage".to_string());
                    }
                    _ => {}
                }
                check_finite(rect, "invalid link rect")?;
                if rect[2] <= rect[0] || rect[3] <= rect[1] {
                    return Err("invalid link rect".to_string());
                }
                if let Some(target) = go_to_page
                    && *target >= page_count
                {
                    return Err(format!(
                        "goToPage {target} out of range ({page_count} pages)"
                    ));
                }
            }
            DrawOp::Path {
                page,
                segments,
                fill,
                stroke,
                stroke_width,
                opacity,
                ..
            } => {
                check_page(*page, page_count)?;
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
    Ok(())
}

/// Gather the characters used per embedded-font id across every text draw op.
/// Callers combine this with other sources (e.g. fill values) before building.
pub(crate) fn draw_used_chars(ops: &[DrawOp]) -> HashMap<usize, BTreeSet<char>> {
    let mut used_per_font: HashMap<usize, BTreeSet<char>> = HashMap::new();
    for op in ops {
        if let DrawOp::Text {
            font_id: Some(i),
            text,
            ..
        } = op
        {
            used_per_font.entry(*i).or_default().extend(text.chars());
        }
    }
    used_per_font
}

/// Build each embedded font once (subset + Type0 graph), keyed by font id.
/// Mirrors the create-path pre-pass in `create.rs`; `doc_add` lets callers add
/// objects to whichever document is being written (a fresh `Document` at
/// create time, or `IncrementalDocument.new_document` at apply time).
pub(crate) fn build_document_fonts(
    doc_add: &mut dyn FnMut(Object) -> ObjectId,
    font_descs: &[FontDesc],
    fonts_blob: &[u8],
    used_per_font: &HashMap<usize, BTreeSet<char>>,
) -> Result<HashMap<usize, (ObjectId, BuiltFont)>, String> {
    let mut embedded_fonts: HashMap<usize, (ObjectId, BuiltFont)> = HashMap::new();
    // Deterministic build order by font id.
    let mut ids: Vec<usize> = used_per_font.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        if id >= font_descs.len() {
            return Err(format!("font id {id} out of range"));
        }
        let fd = &font_descs[id];
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        if end > fonts_blob.len() {
            return Err("font range out of bounds".to_string());
        }
        let bytes = &fonts_blob[fd.offset..end];
        let input = EmbeddedFontInput {
            data: bytes,
            subset: fd.subset,
            used_chars: used_per_font.get(&id).cloned().unwrap_or_default(),
        };
        let built = build_embedded_font(doc_add, &input)?;
        embedded_fonts.insert(id, built);
    }
    Ok(embedded_fonts)
}

/// Emit one merged content stream per touched page (text/image/shape ops in
/// order) and splice it into each page's `/Contents`, registering fonts,
/// xobjects and ExtGStates as needed.
fn emit_page_ops(
    inc: &mut IncrementalDocument,
    page_ops: &[(usize, Vec<&DrawOp>)],
    embedded_fonts: &std::collections::HashMap<usize, (ObjectId, BuiltFont)>,
    font_descs: &[FontDesc],
    fonts: &[u8],
    images: &[u8],
) -> Result<(), String> {
    // Create q and Q streams once, shared across all touched pages
    let q_id = inc.new_document.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"q\n".to_vec(),
    )));
    let q_ref_id = inc.new_document.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"Q\n".to_vec(),
    )));

    // Pre-create font objects (one per unique font used, keyed by STANDARD_14 index)
    let mut font_cache: std::collections::HashMap<usize, ObjectId> =
        std::collections::HashMap::new();
    // Global image counter for unique XObject keys across all pages
    let mut img_counter: usize = 0;

    // Global page-embed counter for unique BPp Form-XObject keys across all pages
    let mut page_embed_counter: usize = 0;

    // Global ExtGState counter for unique BPG keys across all pages
    let mut gs_counter: usize = 0;

    // Process each touched page
    for (page_idx, page_op_list) in page_ops {
        // Build one stream containing a separate BT...ET block per text op
        // and q...cm...Do...Q block per image op.
        // Each BT resets the text matrix to identity, making each op's Td
        // absolute rather than relative to the previous line origin.
        let mut stream_content = Vec::new();

        // Collect xobjects to register on this page: (key, xobject_id)
        let mut xobjects_on_page: Vec<(String, ObjectId)> = Vec::new();

        // Collect ExtGState entries to register on this page: (key, gs_id)
        let mut extgstates_on_page: Vec<(String, ObjectId)> = Vec::new();

        for op in page_op_list {
            match op {
                DrawOp::Text {
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
                    page: _,
                } => {
                    // Register ExtGState for opacity if present
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);

                    // Word-wrap server-side when maxWidth is set: one source of
                    // truth (vs. the old per-word measurement across the WASM
                    // boundary in the TS layer).
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
                        let (_type0_id, built) = embedded_fonts.get(id).unwrap();
                        let gids_per_line: Vec<Vec<u16>> = text
                            .split('\n')
                            .map(|line| {
                                line.chars()
                                    .filter_map(|c| built.gid_for.get(&c).copied())
                                    .collect()
                            })
                            .collect();
                        emit_text_block_cid(
                            &mut stream_content,
                            &format!("BPE{id}"),
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
                        let font_idx = standard_14_index(font.as_str()).unwrap();

                        // Ensure font object exists
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            font_cache.entry(font_idx)
                        {
                            let fid = inc
                                .new_document
                                .add_object(Object::Dictionary(font_dict(font)));
                            e.insert(fid);
                        }

                        // One self-contained BT...ET block per op; BT resets the
                        // text matrix to identity so Td gives absolute positioning.
                        emit_text_block(
                            &mut stream_content,
                            &format!("BPF{font_idx}"),
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
                DrawOp::Image {
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
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    let end = image_offset + image_length;
                    let bytes = &images[*image_offset..end];
                    let img = crate::appearance::signature_image(bytes)?;
                    let xid = crate::appearance::build_image_xobjects(img, &mut |o| {
                        inc.new_document.add_object(o)
                    });
                    let key = format!("BPI{img_counter}");
                    img_counter += 1;
                    emit_image_op(
                        &mut stream_content,
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
                    xobjects_on_page.push((key, xid));
                }
                DrawOp::Page {
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
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    let end = image_offset + image_length;
                    let src = &images[*image_offset..end];
                    // embed_page_as_xobject borrows new_document mutably; the draw
                    // stream is a separate Vec, so no borrow conflict here.
                    let (xid, bw, bh) =
                        crate::embed::embed_page_as_xobject(&mut inc.new_document, src, *src_page)?;
                    let key = format!("BPp{page_embed_counter}");
                    page_embed_counter += 1;
                    // Form BBox is [0 0 bw bh], so scale by width/bw, height/bh.
                    stream_content.extend_from_slice(b"q\n");
                    if let Some(k) = gs_key.as_deref() {
                        writeln!(stream_content, "/{k} gs").unwrap();
                    }
                    emit_placement(
                        &mut stream_content,
                        *x,
                        *y,
                        *width / bw,
                        *height / bh,
                        *rotate,
                        *x_skew,
                        *y_skew,
                    );
                    writeln!(stream_content, "/{key} Do").unwrap();
                    stream_content.extend_from_slice(b"Q\n");
                    xobjects_on_page.push((key, xid));
                }
                DrawOp::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    thickness,
                    color,
                    opacity,
                    dash,
                    dash_phase,
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    emit_line(
                        &mut stream_content,
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
                DrawOp::Rectangle {
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
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    emit_rectangle(
                        &mut stream_content,
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
                DrawOp::Ellipse {
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
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    emit_ellipse(
                        &mut stream_content,
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
                DrawOp::Path {
                    segments,
                    fill,
                    stroke,
                    stroke_width,
                    opacity,
                    dash,
                    dash_phase,
                    page: _,
                } => {
                    let gs_key =
                        alloc_opacity_gs(opacity, &mut gs_counter, &mut extgstates_on_page, inc);
                    emit_path(
                        &mut stream_content,
                        gs_key.as_deref(),
                        segments,
                        *fill,
                        *stroke,
                        *stroke_width,
                        dash,
                        *dash_phase,
                    );
                }
                // Mutation/annotation ops produce no content; applied to the
                // page dict after clone.
                DrawOp::SetRotation { .. } | DrawOp::SetMediaBox { .. } | DrawOp::Link { .. } => {}
            }
        }

        // Get the page ObjectId from the previous document (page_idx is 0-based)
        let page_id = {
            let prev = inc.get_prev_documents();
            let mut sorted_pages: Vec<(u32, ObjectId)> = prev.get_pages().into_iter().collect();
            sorted_pages.sort_by_key(|(num, _)| *num);
            sorted_pages
                .get(*page_idx)
                .map(|(_, id)| *id)
                .ok_or_else(|| format!("page index {page_idx} out of range"))?
        };

        // Clone the page into the new document so we can mutate it
        inc.opt_clone_object_to_new_document(page_id)
            .map_err(|e| e.to_string())?;

        // Apply page-dict mutation ops (Rotate / MediaBox) to the cloned page.
        for op in page_op_list {
            match op {
                DrawOp::SetRotation { degrees, .. } => {
                    let norm = ((degrees % 360) + 360) % 360;
                    dict_mut(inc, page_id)?.set("Rotate", Object::Integer(norm));
                }
                DrawOp::SetMediaBox { media_box, .. } => {
                    let arr: Vec<Object> = media_box.iter().map(|v| Object::Real(*v)).collect();
                    dict_mut(inc, page_id)?.set("MediaBox", Object::Array(arr));
                }
                _ => {}
            }
        }

        // Append link annotations to the cloned page's /Annots.
        for op in page_op_list {
            if let DrawOp::Link {
                rect,
                uri,
                go_to_page,
                ..
            } = op
            {
                // Resolve the destination page id (for goToPage) the same way we
                // resolve the current page id: from the prev doc's sorted pages.
                let dest_page =
                    match go_to_page {
                        Some(target) => {
                            let prev = inc.get_prev_documents();
                            let mut sorted: Vec<(u32, ObjectId)> =
                                prev.get_pages().into_iter().collect();
                            sorted.sort_by_key(|(num, _)| *num);
                            Some(sorted.get(*target).map(|(_, id)| *id).ok_or_else(|| {
                                format!("link goToPage index {target} out of range")
                            })?)
                        }
                        None => None,
                    };
                let annot = link_annot_dict(*rect, uri.as_deref(), dest_page);
                let annot_id = inc.new_document.add_object(Object::Dictionary(annot));
                append_annot_to_page(inc, page_id, annot_id)?;
            }
        }

        // Empty-content guard: a page touched only by mutation ops produces no
        // draw content — skip the draw-stream creation and Contents rewrite
        // entirely (the page was still cloned + mutated above).
        if !stream_content.is_empty() {
            let draw_id = inc.new_document.add_object(Object::Stream(Stream::new(
                Dictionary::new(),
                stream_content,
            )));

            // Build new Contents array: [q_ref, ...original, Q_ref, draw_ref...]
            // Read and clone the existing Contents value first so the borrow ends
            // before we mutate inc.new_document (needed for Issue 2 below).
            let existing_contents: Option<Object> =
                dict_mut(inc, page_id)?.get(b"Contents").ok().cloned();

            // Issue 3: missing /Contents is valid (blank page); treat as empty.
            // Issue 2: a direct Stream in /Contents must be made indirect —
            //          streams must be indirect objects when referenced from an
            //          array. Promote it by adding it to new_document.
            let mut arr: Vec<Object> = match existing_contents {
                Some(Object::Array(a)) => a,
                Some(Object::Stream(s)) => {
                    // Direct stream — make it indirect so the array only holds refs.
                    let indirect_id = inc.new_document.add_object(Object::Stream(s));
                    vec![Object::Reference(indirect_id)]
                }
                Some(single) => vec![single],
                None => Vec::new(), // missing /Contents — blank page
            };
            // Wrap: q, ...original, Q, draw...
            arr.insert(0, Object::Reference(q_id));
            arr.push(Object::Reference(q_ref_id));
            arr.push(Object::Reference(draw_id));
            dict_mut(inc, page_id)?.set("Contents", Object::Array(arr));
        }

        // Collect unique fonts used on this page (standard-14 vs embedded)
        let mut fonts_on_page: Vec<(usize, String)> = Vec::new();
        let mut embedded_on_page: Vec<usize> = Vec::new();
        for op in page_op_list {
            if let DrawOp::Text { font, font_id, .. } = op {
                if let Some(id) = font_id {
                    if !embedded_on_page.contains(id) {
                        embedded_on_page.push(*id);
                    }
                } else {
                    let idx = standard_14_index(font.as_str()).unwrap();
                    if !fonts_on_page.iter().any(|(i, _)| *i == idx) {
                        fonts_on_page.push((idx, font.clone()));
                    }
                }
            }
        }

        for (font_idx, _font_name) in &fonts_on_page {
            let font_obj_id = *font_cache.get(font_idx).unwrap();
            let key = format!("BPF{font_idx}");
            register_font(inc, page_id, &key, font_obj_id)?;
        }

        // Register embedded Type0 fonts in the page's /Font subdict (key BPE{id}).
        for id in &embedded_on_page {
            let (type0_id, _) = embedded_fonts.get(id).unwrap();
            let key = format!("BPE{id}");
            register_font(inc, page_id, &key, *type0_id)?;
        }

        // Register XObjects for image ops on this page
        for (key, xid) in &xobjects_on_page {
            register_xobject(inc, page_id, key, *xid)?;
        }

        // Register ExtGState entries for shape ops with opacity on this page
        for (key, gs_id) in &extgstates_on_page {
            register_extgstate(inc, page_id, key, *gs_id)?;
        }
    }
    Ok(())
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
            resolve_and_set_subdict(inc, id, b"Font", key, font_id)?;
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            if !page.has(b"Resources") {
                page.set("Resources", Object::Dictionary(Dictionary::new()));
            }
            // Check if /Font sub-dict is an indirect reference stored inline in Resources
            let font_sub_ref = page
                .get_mut(b"Resources")
                .and_then(Object::as_dict_mut)
                .ok()
                .and_then(|res| res.get(b"Font").ok().cloned())
                .and_then(|obj| {
                    if let Object::Reference(id) = obj {
                        Some(id)
                    } else {
                        None
                    }
                });
            if let Some(sub_id) = font_sub_ref {
                // Clone the indirect Font dict and mutate it directly
                inc.opt_clone_object_to_new_document(sub_id)
                    .map_err(|e| e.to_string())?;
                let font_dict_obj = inc
                    .new_document
                    .get_object_mut(sub_id)
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                font_dict_obj.set(key.as_bytes().to_vec(), Object::Reference(font_id));
            } else {
                let res = dict_mut(inc, page_id)?
                    .get_mut(b"Resources")
                    .and_then(Object::as_dict_mut)
                    .map_err(|e| e.to_string())?;
                set_font(res, key, font_id);
            }
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

/// Append `Reference(annot_id)` to a cloned page's `/Annots`, handling the
/// three storage forms: inline array (push), indirect reference to an array
/// (clone into new_document then push), or absent (create a new array).
pub(crate) fn append_annot_to_page(
    inc: &mut IncrementalDocument,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> Result<(), String> {
    // Is /Annots an indirect reference?
    let annots_ref = match dict_mut(inc, page_id)?.get(b"Annots") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    match annots_ref {
        Some(arr_id) => {
            // Clone the indirect array object into new_document and push.
            inc.opt_clone_object_to_new_document(arr_id)
                .map_err(|e| e.to_string())?;
            let arr = inc
                .new_document
                .get_object_mut(arr_id)
                .and_then(Object::as_array_mut)
                .map_err(|e| e.to_string())?;
            arr.push(Object::Reference(annot_id));
        }
        None => {
            let page = dict_mut(inc, page_id)?;
            match page.get_mut(b"Annots") {
                Ok(Object::Array(arr)) => arr.push(Object::Reference(annot_id)),
                _ => page.set("Annots", Object::Array(vec![Object::Reference(annot_id)])),
            }
        }
    }
    Ok(())
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

    fn ops(json: &str, images: &[u8]) -> Vec<u8> {
        apply_draw_ops_json(FICHA, json, images, &[], "[]", false).unwrap()
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
        let content = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        String::from_utf8_lossy(&content).into_owned()
    }

    fn tiny_png() -> &'static [u8] {
        // 1×1 RGBA PNG (color_type=6) — used by existing tests; also serves as the alpha fixture
        &[
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
            0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
            0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }

    /// 1×1 RGBA PNG — explicit alias so new tests are self-documenting.
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

    /// Walk page 0's Resources/XObject and return true if any entry resolves to
    /// a stream with /Subtype /Form.
    fn page0_has_form_xobject(out: &[u8]) -> bool {
        let doc = Document::load_mem(out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let res = match page.get(b"Resources") {
            Ok(lopdf::Object::Reference(r)) => doc.get_dictionary(*r).unwrap(),
            Ok(lopdf::Object::Dictionary(d)) => d,
            _ => return false,
        };
        let xo = match res.get(b"XObject") {
            Ok(lopdf::Object::Reference(r)) => doc.get_dictionary(*r).unwrap(),
            Ok(lopdf::Object::Dictionary(d)) => d,
            _ => return false,
        };
        for (_k, v) in xo.iter() {
            let id = match v {
                lopdf::Object::Reference(r) => *r,
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

    #[test]
    fn draws_embedded_pdf_page() {
        // Embed page 0 of FICHA (passed as the source via the images blob) onto FICHA page 0.
        let src = FICHA;
        let len = src.len();
        let json = format!(
            r#"[{{"op":"page","page":0,"x":0,"y":0,"width":300,"height":400,"imageOffset":0,"imageLength":{len},"srcPage":0}}]"#
        );
        let out = apply_draw_ops_json(FICHA, &json, src, &[], "[]", false).unwrap();
        assert!(
            page0_has_form_xobject(&out),
            "page 0 must carry a Form XObject"
        );
        let content = last_draw_stream_content(&out);
        assert!(
            content.contains("/BPp0 Do"),
            "draw stream missing /BPp0 Do: {content}"
        );
    }

    #[test]
    fn set_rotation_persists() {
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setRotation","page":0,"degrees":90}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let rot = doc
            .get_dictionary(pid)
            .unwrap()
            .get(b"Rotate")
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(rot, 90);
    }

    #[test]
    fn set_rotation_normalizes_negative() {
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setRotation","page":0,"degrees":-90}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert_eq!(
            doc.get_dictionary(pid)
                .unwrap()
                .get(b"Rotate")
                .unwrap()
                .as_i64()
                .unwrap(),
            270
        );
    }

    #[test]
    fn set_rotation_rejects_non_multiple_of_90() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setRotation","page":0,"degrees":45}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.unwrap_err().contains("90"));
    }

    #[test]
    fn set_media_box_changes_dimensions() {
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setMediaBox","page":0,"box":[0,0,200,300]}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
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
        assert!((mb[3].as_float().unwrap() - 300.0).abs() < 0.5);
    }

    #[test]
    fn set_media_box_rejects_inverted() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setMediaBox","page":0,"box":[100,0,50,300]}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.is_err());
    }

    #[test]
    fn rotation_only_page_has_no_empty_draw_stream_corruption() {
        // a page with only a mutation op must still reload cleanly
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"setRotation","page":0,"degrees":180}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental
        assert!(Document::load_mem(&out).is_ok());
    }

    #[test]
    fn cid_text_block_emits_hex_glyph_string() {
        let mut out = Vec::new();
        // two lines, gids per line
        emit_text_block_cid(
            &mut out,
            "BPE0",
            50.0,
            700.0,
            12.0,
            [0.0, 0.0, 0.0],
            &[vec![0x0048u16, 0x00E9u16], vec![0x0041u16]],
            None,
            None,
            None,
        );
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("/BPE0 12 Tf"), "content: {s}");
        assert!(
            s.contains("<0048 00E9>") || s.contains("<004800E9>"),
            "hex glyph string missing: {s}"
        );
        assert_eq!(s.matches(" Tj").count(), 2, "one Tj per line: {s}");
        assert!(s.contains("BT") && s.contains("ET"));
    }

    #[test]
    fn draws_embedded_font_text() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, FONT.len());
        let ops = r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"fontId":0,"color":[0,0,0],"text":"Héllo"}]"#;
        let out = apply_draw_ops_json(FICHA, ops, &[], FONT, &fonts_json, false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let res_dict = doc.get_dictionary(first).unwrap();
        // a BPE* (embedded) font key is registered in Resources/Font
        let res = match res_dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        let fonts = match res.get(b"Font").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Font dict"),
        };
        assert!(
            fonts.iter().any(|(k, _)| k.starts_with(b"BPE")),
            "expected a BPE* embedded font key in Font resources"
        );
        let s = last_draw_stream_content(&out);
        assert!(s.contains("Tf") && s.contains(" Tj"));
        assert!(
            s.contains('<') && s.contains('>'),
            "should emit hex glyph string: {s}"
        );
    }

    #[test]
    fn output_is_incremental() {
        let out = ops(
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#,
            &[],
        );
        assert_eq!(&out[..FICHA.len()], FICHA);
        assert!(out.len() > FICHA.len());
    }

    #[test]
    fn page_contents_grow_and_balance() {
        let out = ops(
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"Hello"}]"#,
            &[],
        );
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
        let out = ops(
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Times-Bold","color":[0,0,0],"text":"x"}]"#,
            &[],
        );
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
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"text","page":999,"x":0,"y":0,"size":10,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_unknown_font() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"text","page":0,"x":0,"y":0,"size":10,"font":"Comic Sans","color":[0,0,0],"text":"x"}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.unwrap_err().contains("font"));
    }

    #[test]
    fn multiline_emits_multiple_tj() {
        let out = ops(
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"a\nb"}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert!(s.matches(" Tj").count() == 2, "content was: {s}");
    }

    #[test]
    fn ops_on_same_page_are_absolutely_positioned() {
        let out = ops(
            r#"[
            {"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"first"},
            {"op":"text","page":0,"x":200,"y":300,"size":12,"font":"Helvetica","color":[0,0,0],"text":"second"}
        ]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert_eq!(
            s.matches("BT").count(),
            2,
            "one BT/ET block per op, content: {s}"
        );
        assert!(s.contains("50 700 Td"));
        assert!(
            s.contains("200 300 Td"),
            "second op must be absolutely positioned, content: {s}"
        );
    }

    #[test]
    fn draws_image_on_page() {
        let png = tiny_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"image","page":0,"x":50,"y":50,"width":100,"height":80,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = apply_draw_ops_json(FICHA, &json, png, &[], "[]", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();

        // Verify XObject is registered in Resources
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        let xobjs = match res.get(b"XObject").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected XObject dict"),
        };
        let bpi_entry = xobjs.iter().find(|(k, _)| k.starts_with(b"BPI"));
        assert!(
            bpi_entry.is_some(),
            "expected a BPI* key in XObject resources"
        );

        // Verify the XObject itself is an Image
        let xobj_ref = bpi_entry.unwrap().1.as_reference().unwrap();
        let xobj_stream = doc.get_object(xobj_ref).unwrap().as_stream().unwrap();
        let subtype = xobj_stream.dict.get(b"Subtype").unwrap();
        assert_eq!(subtype.as_name().unwrap(), b"Image");

        // Verify the draw stream references /BPI0 Do
        let s = last_draw_stream_content(&out);
        assert!(
            s.contains("/BPI0 Do"),
            "draw stream should contain '/BPI0 Do', got: {s}"
        );
    }

    #[test]
    fn loaded_image_with_alpha_has_smask() {
        let png = tiny_rgba_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"image","page":0,"x":10,"y":10,"width":20,"height":20,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = apply_draw_ops_json(FICHA, &json, png, &[], "[]", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        let xobjs = match res.get(b"XObject").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected XObject dict"),
        };
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
    fn loaded_opaque_image_has_no_smask() {
        let png = tiny_rgb_png();
        let len = png.len();
        let json = format!(
            r#"[{{"op":"image","page":0,"x":10,"y":10,"width":20,"height":20,"imageOffset":0,"imageLength":{len}}}]"#
        );
        let out = apply_draw_ops_json(FICHA, &json, png, &[], "[]", false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        let xobjs = match res.get(b"XObject").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected XObject dict"),
        };
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
    fn image_out_of_bounds_errors() {
        let png = tiny_png();
        let len = png.len();
        // offset + length exceeds images blob length
        let json = format!(
            r#"[{{"op":"image","page":0,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{}}}]"#,
            len + 1
        );
        let r = apply_draw_ops_json(FICHA, &json, png, &[], "[]", false);
        assert!(r.unwrap_err().contains("out of bounds"));
    }

    #[test]
    fn invalid_image_bytes_error() {
        let bad_bytes = b"not an image";
        let json = format!(
            r#"[{{"op":"image","page":0,"x":0,"y":0,"width":10,"height":10,"imageOffset":0,"imageLength":{}}}]"#,
            bad_bytes.len()
        );
        let r = apply_draw_ops_json(FICHA, &json, bad_bytes, &[], "[]", false);
        assert!(r.is_err(), "expected error for invalid image bytes");
    }

    #[test]
    fn draws_line() {
        let out = ops(
            r#"[{"op":"line","page":0,"x1":50,"y1":100,"x2":250,"y2":100,"thickness":2,"color":[1,0,0]}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert!(s.contains(" m"), "missing m operator, content: {s}");
        assert!(s.contains(" l"), "missing l operator, content: {s}");
        assert!(s.contains("S"), "missing S operator, content: {s}");
        assert!(s.contains("1 0 0 RG"), "missing stroke color, content: {s}");
    }

    #[test]
    fn draws_rectangle_fill_and_border() {
        let out = ops(
            r#"[{"op":"rectangle","page":0,"x":50,"y":100,"width":200,"height":80,"color":[0.9,0.9,0.9],"borderColor":[0,0,0],"borderWidth":1}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert!(s.contains(" re"), "missing re operator, content: {s}");
        assert!(s.contains("B"), "missing B paint operator, content: {s}");
    }

    #[test]
    fn rectangle_fill_only_uses_f() {
        let out = ops(
            r#"[{"op":"rectangle","page":0,"x":10,"y":10,"width":50,"height":30,"color":[0,0,1]}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert!(s.contains(" re"), "missing re operator, content: {s}");
        // Should have standalone "f" paint (not "B")
        assert!(
            s.split_whitespace().any(|w| w == "f"),
            "missing standalone f paint operator, content: {s}"
        );
        assert!(
            !s.contains('B'),
            "should not have B when no border, content: {s}"
        );
    }

    #[test]
    fn draws_ellipse() {
        let out = ops(
            r#"[{"op":"ellipse","page":0,"x":150,"y":140,"xScale":100,"yScale":40,"color":[0,0,1],"borderColor":[0,0,0],"borderWidth":1}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        assert!(
            s.matches(" c").count() >= 4,
            "expected >= 4 cubic bezier segments, content: {s}"
        );
        assert!(
            s.contains("B") || s.contains("f") || s.contains("S"),
            "missing paint operator, content: {s}"
        );
    }

    #[test]
    fn opacity_registers_extgstate() {
        let out = ops(
            r#"[{"op":"rectangle","page":0,"x":50,"y":100,"width":200,"height":80,"color":[0.9,0.9,0.9],"opacity":0.5}]"#,
            &[],
        );
        let doc = Document::load_mem(&out).unwrap();
        let (_, first) = doc.get_pages().into_iter().next().unwrap();
        let dict = doc.get_dictionary(first).unwrap();
        // Resolve Resources
        let res = match dict.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        // Resolve ExtGState
        let extgstate = match res.get(b"ExtGState").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected ExtGState dict"),
        };
        // BPG0 must exist
        let bpg0_ref = extgstate
            .get(b"BPG0")
            .expect("BPG0 not found in ExtGState resources");
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
        // Content must reference /BPG0 gs
        let s = last_draw_stream_content(&out);
        assert!(s.contains("/BPG0 gs"), "content missing /BPG0 gs, got: {s}");
    }

    #[test]
    fn opacity_out_of_range_errors() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"rectangle","page":0,"x":0,"y":0,"width":10,"height":10,"color":[0,0,0],"opacity":1.5}]"#,
            &[],
            &[],
            "[]", false
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("opacity"),
            "expected opacity error, got: {err}"
        );
    }

    #[test]
    fn shapes_compose_with_text_in_order() {
        let out = ops(
            r#"[
                {"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"hello"},
                {"op":"rectangle","page":0,"x":50,"y":600,"width":100,"height":50,"color":[1,0,0]}
            ]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        let tj_pos = s.find(") Tj").expect("missing Tj in content");
        let re_pos = s.find(" re").expect("missing re in content");
        assert!(
            tj_pos < re_pos,
            "text (Tj at {tj_pos}) should appear before rectangle (re at {re_pos}), content: {s}"
        );
    }

    // Finding 2 regression: page whose /Resources/Font is an indirect object still gets
    // the new BPF font registered. We build an in-memory PDF where /Resources/Font is
    // stored as an indirect reference (lopdf stores it inline by default, so we promote it).
    #[test]
    fn font_registered_when_resources_font_is_indirect() {
        use lopdf::{Dictionary, Document, Object, Stream, dictionary};

        // Build a minimal 1-page PDF with /Resources/Font as an indirect object
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Indirect Font dict (existing entry to verify we don't lose it)
        let existing_font_id = doc.add_object(Object::Dictionary(font_dict("Helvetica")));
        let mut font_res = Dictionary::new();
        font_res.set("ExistingFont", Object::Reference(existing_font_id));
        let font_res_id = doc.add_object(Object::Dictionary(font_res));

        let mut resources = Dictionary::new();
        resources.set("Font", Object::Reference(font_res_id));

        let content_id =
            doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"".to_vec())));
        let page_dict = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0), Object::Real(0.0),
                Object::Real(595.0), Object::Real(842.0),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1i64),
        };
        doc.set_object(pages_id, Object::Dictionary(pages_dict));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut base = Vec::new();
        doc.save_to(&mut base).unwrap();

        // Apply a drawText op — this exercises register_font with an indirect Font sub-dict
        let out = apply_draw_ops_json(
            &base,
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":12,"font":"Helvetica","color":[0,0,0],"text":"hi"}]"#,
            &[],
            &[],
            "[]", false
        )
        .expect("apply_draw_ops_json should succeed");

        // Reload and verify both the original ExistingFont and new BPF font are present
        let doc2 = Document::load_mem(&out).unwrap();
        let (_, first) = doc2.get_pages().into_iter().next().unwrap();
        let page = doc2.get_dictionary(first).unwrap();
        let res = match page.get(b"Resources").unwrap() {
            lopdf::Object::Reference(r) => doc2.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Resources dict"),
        };
        let fonts = match res.get(b"Font").unwrap() {
            lopdf::Object::Reference(r) => doc2.get_object(*r).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            _ => panic!("expected Font dict"),
        };
        assert!(
            fonts.iter().any(|(k, _)| k.starts_with(b"BPF")),
            "new BPF font not found; fonts: {:?}",
            fonts
                .iter()
                .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                .collect::<Vec<_>>()
        );
        assert!(
            fonts.has(b"ExistingFont"),
            "original ExistingFont was lost after drawText"
        );
    }

    // Finding 4: ellipse content must contain closepath 'h' before the paint operator
    #[test]
    fn ellipse_content_contains_closepath() {
        let out = ops(
            r#"[{"op":"ellipse","page":0,"x":150,"y":140,"xScale":100,"yScale":40,"color":[0,0,1],"borderColor":[0,0,0],"borderWidth":1}]"#,
            &[],
        );
        let s = last_draw_stream_content(&out);
        // Find last 'c' curve and ensure 'h' appears before the paint op
        let h_pos = s
            .find("\nh\n")
            .expect("missing closepath 'h' in ellipse content");
        let paint_pos = s
            .rfind('B')
            .or_else(|| s.rfind('f'))
            .or_else(|| s.rfind('S'))
            .expect("missing paint operator");
        assert!(
            h_pos < paint_pos,
            "closepath 'h' (at {h_pos}) must appear before paint op (at {paint_pos}), content: {s}"
        );
    }

    // Finding 5: ellipse with xScale=0 must error with "must be > 0"
    #[test]
    fn ellipse_zero_scale_errors() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"ellipse","page":0,"x":100,"y":100,"xScale":0,"yScale":50,"color":[0,0,1]}]"#,
            &[],
            &[],
            "[]", false
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("must be > 0"),
            "expected 'must be > 0' error for xScale=0, got: {err}"
        );
    }

    // Finding 5: rectangle with zero width must error
    #[test]
    fn rectangle_zero_width_errors() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"rectangle","page":0,"x":10,"y":10,"width":0,"height":30,"color":[0,0,1]}]"#,
            &[],
            &[],
            "[]", false
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("must be > 0"),
            "expected 'must be > 0' error for width=0, got: {err}"
        );
    }

    // ── M32: link annotations ──────────────────────────────────────────────

    /// Resolve a page's /Annots into the dictionaries of its entries, handling
    /// both an inline array and an indirect reference to the array, and
    /// dereferencing each entry.
    fn resolve_annots<'a>(doc: &'a Document, page: &'a Dictionary) -> Vec<&'a Dictionary> {
        let arr: Vec<Object> = match page.get(b"Annots") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(r)) => match doc.get_object(*r).and_then(|o| o.as_array()) {
                Ok(a) => a.clone(),
                Err(_) => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let mut result: Vec<&'a Dictionary> = Vec::new();
        for entry in arr {
            match entry {
                Object::Reference(r) => {
                    if let Ok(d) = doc.get_object(r).and_then(|o| o.as_dict()) {
                        result.push(d);
                    }
                }
                Object::Dictionary(_) => {
                    // Inline annot dicts: look them up again by re-reading the page
                    // would not give a 'a ref; created docs use references, so skip.
                }
                _ => {}
            }
        }
        result
    }

    #[test]
    fn appends_uri_link_annotation() {
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"link","page":0,"rect":[50,50,200,80],"uri":"https://example.com"}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(pid).unwrap();
        let annots = resolve_annots(&doc, page);
        let link = annots
            .iter()
            .find(|a| {
                a.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Link".as_ref())
            })
            .expect("a /Link annot");
        assert_eq!(link.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
        let a = link.get(b"A").unwrap().as_dict().unwrap();
        assert_eq!(a.get(b"S").unwrap().as_name().unwrap(), b"URI");
        let uri = a.get(b"URI").unwrap().as_str().unwrap();
        assert_eq!(uri, b"https://example.com");
    }

    #[test]
    fn appends_goto_link_with_dest() {
        let out = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"link","page":0,"rect":[10,10,100,40],"goToPage":0}]"#,
            &[],
            &[],
            "[]", false
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        let annots = resolve_annots(&doc, doc.get_dictionary(pid).unwrap());
        let link = annots
            .iter()
            .find(|a| a.has(b"Dest"))
            .expect("a link with /Dest");
        assert!(link.get(b"Dest").unwrap().as_array().is_ok());
    }

    #[test]
    fn link_rejects_both_uri_and_goto() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"link","page":0,"rect":[0,0,10,10],"uri":"x","goToPage":0}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.is_err());
    }

    #[test]
    fn link_rejects_neither() {
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"link","page":0,"rect":[0,0,10,10]}]"#,
            &[],
            &[],
            "[]", false
        );
        assert!(r.is_err());
    }

    #[test]
    fn draws_path_with_fill_and_stroke() {
        let json = r#"[{"op":"path","page":0,"segments":[
            {"t":"m","x":50,"y":50},{"t":"l","x":150,"y":50},
            {"t":"c","x1":160,"y1":60,"x2":160,"y2":140,"x":150,"y":150},
            {"t":"z"}],"fill":[1,0,0],"stroke":[0,0,0],"strokeWidth":2}]"#;
        let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(s.contains("50 50 m"), "content: {s}");
        assert!(s.contains(" l"), "content: {s}");
        assert!(s.contains(" c"), "content: {s}");
        assert!(s.contains("h\n") || s.contains(" h"), "close: {s}");
        assert!(s.contains('B'), "fill+stroke should paint with B: {s}");
        assert!(s.contains("1 0 0 rg"), "fill color: {s}");
        assert!(s.contains("2 w"), "stroke width: {s}");
    }

    #[test]
    fn path_fill_only_uses_f() {
        let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":10,"y":0},{"t":"l","x":10,"y":10},{"t":"z"}],"fill":[0,0,1]}]"#;
        let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(
            s.split_whitespace().any(|w| w == "f"),
            "fill-only path should paint with f: {s}"
        );
    }

    #[test]
    fn path_opacity_registers_extgstate() {
        let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":10,"y":10}],"stroke":[0,0,0],"opacity":0.5}]"#;
        let out = apply_draw_ops_json(FICHA, json, &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(
            s.contains("/BPG"),
            "opacity should reference an ExtGState: {s}"
        );
    }

    #[test]
    fn path_rejects_non_finite_coord() {
        // serde_json rejects 1e999 (inf) before we even validate, so either way we get an error
        let json = r#"[{"op":"path","page":0,"segments":[{"t":"m","x":0,"y":0},{"t":"l","x":1e999,"y":0}],"stroke":[0,0,0]}]"#;
        assert!(apply_draw_ops_json(FICHA, json, &[], &[], "[]", false).is_err());
    }

    #[test]
    fn path_rejects_empty_segments() {
        let json = r#"[{"op":"path","page":0,"segments":[],"stroke":[0,0,0]}]"#;
        let err = apply_draw_ops_json(FICHA, json, &[], &[], "[]", false).unwrap_err();
        assert!(
            err.contains("segment"),
            "expected segment error, got: {err}"
        );
    }

    // ── M34: text rotation + opacity ─────────────────────────────────────────

    #[test]
    fn rotated_text_emits_matrix() {
        let out = apply_draw_ops_json(FICHA,
            r#"[{"op":"text","page":0,"x":100,"y":100,"size":12,"font":"Helvetica","color":[0,0,0],"text":"hi","rotate":90}]"#,
            &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(s.contains(" cm"), "rotation must emit a cm matrix: {s}");
        assert!(
            s.contains("q") && s.contains("Q"),
            "rotated text wrapped in q/Q: {s}"
        );
        assert!(
            s.contains("0 0 Td"),
            "rotated text uses Td 0 0 (cm positions): {s}"
        );
    }

    #[test]
    fn translucent_text_registers_extgstate() {
        let out = apply_draw_ops_json(FICHA,
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"wm","opacity":0.3}]"#,
            &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(
            s.contains("/BPG"),
            "opacity text references an ExtGState: {s}"
        );
    }

    #[test]
    fn plain_text_unchanged_no_wrap() {
        let out = apply_draw_ops_json(FICHA,
            r#"[{"op":"text","page":0,"x":50,"y":700,"size":24,"font":"Helvetica","color":[0,0,0],"text":"x"}]"#,
            &[], &[], "[]", false).unwrap();
        let s = last_draw_stream_content(&out);
        assert!(s.contains("50 700 Td"), "plain text keeps x y Td: {s}");
    }

    /// Regression: a link op whose goToPage index is out of range must return
    /// Err(...) containing "out of range", never panic (which would abort wasm).
    #[test]
    fn link_go_to_page_out_of_range_returns_err() {
        // FICHA is a 1-page PDF; page index 99 is well out of range.
        let r = apply_draw_ops_json(
            FICHA,
            r#"[{"op":"link","page":0,"rect":[10,10,100,30],"goToPage":99}]"#,
            &[],
            &[],
            "[]", false
        );
        let err = r.unwrap_err();
        assert!(
            err.contains("out of range"),
            "expected 'out of range' in error, got: {err}"
        );
    }
}
