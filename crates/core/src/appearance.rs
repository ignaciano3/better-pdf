//! Appearance engine: Helvetica metrics, WinAnsi encoding, and Form-XObject
//! construction for filled text/choice fields.

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use lopdf::{Dictionary, Object, Stream};
use std::fmt::Write;
use std::io::{Read, Write as _};

/// Advance-width table (units/1000 em) for one font, indexed by WinAnsi code.
#[derive(Clone)]
pub struct FontWidths(pub [u16; 224]);

impl FontWidths {
    /// Width of one WinAnsi byte; 556 (Helvetica average) for unknown codes.
    pub fn width(&self, code: u8) -> u16 {
        if code >= 32 {
            let w = self.0[(code - 32) as usize];
            if w != 0 {
                return w;
            }
        }
        556
    }
}

/// Helvetica metrics: the fallback when a font can't be identified.
pub fn helvetica_widths() -> FontWidths {
    FontWidths(crate::font_metrics::HELVETICA)
}

/// Resolve a /BaseFont name to a standard-14 width table, stripping subset
/// prefixes ("ABCDEF+Arial-Bold") and aliasing the common TrueType names
/// (Arial -> Helvetica, Times New Roman -> Times, Courier New -> Courier).
pub fn standard_14_widths(base_font: &str) -> Option<FontWidths> {
    use crate::font_metrics as fm;
    let name = base_font.rsplit('+').next().unwrap_or(base_font);
    let lower = name.to_ascii_lowercase();
    let bold = lower.contains("bold");
    let italic = lower.contains("italic") || lower.contains("oblique");
    let table: &[u16; 224] = if lower.contains("courier") {
        match (bold, italic) {
            (true, true) => &fm::COURIER_BOLDOBLIQUE,
            (true, false) => &fm::COURIER_BOLD,
            (false, true) => &fm::COURIER_OBLIQUE,
            (false, false) => &fm::COURIER,
        }
    } else if lower.contains("times") {
        match (bold, italic) {
            (true, true) => &fm::TIMES_BOLDITALIC,
            (true, false) => &fm::TIMES_BOLD,
            (false, true) => &fm::TIMES_ITALIC,
            (false, false) => &fm::TIMES_ROMAN,
        }
    } else if lower.contains("helvetica") || lower.contains("arial") {
        match (bold, italic) {
            (true, true) => &fm::HELVETICA_BOLDOBLIQUE,
            (true, false) => &fm::HELVETICA_BOLD,
            (false, true) => &fm::HELVETICA_OBLIQUE,
            (false, false) => &fm::HELVETICA,
        }
    } else {
        return None;
    };
    Some(FontWidths(*table))
}

/// Width of a WinAnsi byte string at the given font size (points).
pub fn string_width(bytes: &[u8], size: f32, widths: &FontWidths) -> f32 {
    let units: u32 = bytes.iter().map(|&c| widths.width(c) as u32).sum();
    units as f32 / 1000.0 * size
}

/// Horizontal offset of text of width `tw` within a `box_w`-wide field for the
/// quadding `q` (0/left, 1/center, 2/right), clamped to at least `PAD`.
pub(crate) fn quad_offset(q: i64, box_w: f32, tw: f32) -> f32 {
    match q {
        1 => ((box_w - tw) / 2.0).max(PAD), // center
        2 => (box_w - PAD - tw).max(PAD),   // right
        _ => PAD,                           // left
    }
}

/// Word-wrap WinAnsi `text` so each returned line's `string_width <= avail_w`.
/// Hard breaks (`\n`, with `\r\n` and lone `\r` normalized to `\n`) split first;
/// each resulting paragraph is greedily wrapped on ASCII spaces. A single word
/// wider than `avail_w` is placed on its own line (overflow, no mid-word break).
/// A blank paragraph yields one empty line, so blank lines survive. Mirrors the
/// TypeScript `wrapText` in `src/generate/wrap-text.ts`.
pub fn wrap_lines(text: &[u8], size: f32, avail_w: f32, widths: &FontWidths) -> Vec<Vec<u8>> {
    // Normalize CRLF / lone CR to LF, then split on LF into paragraphs.
    let mut normalized: Vec<u8> = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' {
            normalized.push(b'\n');
            if i + 1 < text.len() && text[i + 1] == b'\n' {
                i += 1;
            }
        } else {
            normalized.push(text[i]);
        }
        i += 1;
    }

    let mut out: Vec<Vec<u8>> = Vec::new();
    for para in normalized.split(|&b| b == b'\n') {
        let words: Vec<&[u8]> = para
            .split(|&b| b == b' ')
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            out.push(Vec::new());
            continue;
        }
        let mut current: Vec<u8> = Vec::new();
        for word in words {
            if current.is_empty() {
                current.extend_from_slice(word);
                continue;
            }
            let mut candidate = current.clone();
            candidate.push(b' ');
            candidate.extend_from_slice(word);
            if string_width(&candidate, size, widths) <= avail_w {
                current = candidate;
            } else {
                out.push(std::mem::take(&mut current));
                current.extend_from_slice(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

/// Word-wrap `text` into `\n`-separated lines, each measuring `<= avail_w` via
/// `measure`. Same algorithm as [`wrap_lines`] but operates on `&str` (Unicode
/// chars) instead of WinAnsi bytes, so it serves both standard-14 and embedded
/// fonts at draw time. Hard breaks (`\n`, with `\r\n`/`\r` normalized to `\n`)
/// split first; each paragraph is greedily wrapped on ASCII spaces; a word
/// wider than `avail_w` gets its own line (overflow, no mid-word break); a blank
/// paragraph yields an empty line so blank lines survive.
pub fn wrap_str(text: &str, avail_w: f32, mut measure: impl FnMut(&str) -> f32) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = Vec::new();
    for para in normalized.split('\n') {
        let words: Vec<&str> = para.split(' ').filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in words {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }
            let candidate = format!("{current} {word}");
            if measure(&candidate) <= avail_w {
                current = candidate;
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines.join("\n")
}

/// Wrap `text` for a standard-14 `font` at `size` so each line fits `avail_w`.
pub fn wrap_standard14(text: &str, font: &str, size: f32, avail_w: f32) -> String {
    let widths = standard_14_widths(font).unwrap_or_else(helvetica_widths);
    wrap_str(text, avail_w, |s| {
        string_width(&encode_winansi(s), size, &widths)
    })
}

/// Width in points of `text` rendered in standard-14 `font` at `size`.
/// Errors if `font` is not a standard-14 base name.
pub fn measure_text_width(font: &str, size: f32, text: &str) -> Result<f32, String> {
    let widths = standard_14_widths(font).ok_or_else(|| format!("unknown font: {font}"))?;
    Ok(string_width(&encode_winansi(text), size, &widths))
}

/// Encode a Rust string to WinAnsi bytes. ASCII maps directly; everything else
/// goes through the generated WinAnsi table; unmappable chars become '?'.
pub fn encode_winansi(s: &str) -> Vec<u8> {
    use crate::font_metrics::WINANSI_FROM_UNICODE;
    s.chars()
        .map(|c| {
            let cp = c as u32;
            if cp < 0x80 {
                return cp as u8;
            }
            match WINANSI_FROM_UNICODE.binary_search_by_key(&cp, |&(u, _)| u) {
                Ok(i) => WINANSI_FROM_UNICODE[i].1,
                Err(_) => b'?',
            }
        })
        .collect()
}

/// Escape bytes for a PDF literal string: backslash, parens.
pub fn escape_pdf_literal(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    for &c in b {
        if c == b'\\' || c == b'(' || c == b')' {
            out.push(b'\\');
        }
        out.push(c);
    }
    out
}

pub(crate) const PAD: f32 = 2.0;
pub(crate) const MAX_AUTO: f32 = 12.0;
pub(crate) const MIN_AUTO: f32 = 4.0;

/// Parsed default-appearance string.
#[derive(Clone)]
pub struct Da {
    pub font: String,
    pub size: f32,
    pub color: String,
}

/// Parse a `/DA` string like "/Helv 0 Tf 0 g". Best-effort; falls back to
/// Helv / size 0 / black on anything unrecognized.
pub fn parse_da(da: &str) -> Da {
    let toks: Vec<&str> = da.split_whitespace().collect();
    let mut font = "Helv".to_string();
    let mut size = 0.0f32;
    let mut color = "0 g".to_string();
    if let Some(i) = toks.iter().position(|&t| t == "Tf") {
        if i >= 2 {
            font = toks[i - 2].trim_start_matches('/').to_string();
            size = toks[i - 1].parse().unwrap_or(0.0);
        }
        let rest: Vec<&str> = toks[i + 1..].to_vec();
        if !rest.is_empty() {
            color = rest.join(" ");
        }
    }
    Da { font, size, color }
}

/// Choose a font size. `da_size > 0` is honored; `0` means auto: cap to the
/// box height, then shrink to fit the box width.
pub fn auto_size(da_size: f32, text: &[u8], avail_w: f32, box_h: f32, widths: &FontWidths) -> f32 {
    if da_size > 0.0 {
        return da_size;
    }
    // One ~2pt vertical breathing margin, capped to a sane max (Acrobat-like).
    let mut size = (box_h - 2.0).clamp(MIN_AUTO, MAX_AUTO);
    let w = string_width(text, size, widths);
    if w > avail_w && w > 0.0 {
        size = (size * avail_w / w).max(MIN_AUTO);
    }
    size
}

/// Build the content stream for a single-line text appearance. `q` is the
/// quadding: 0=left, 1=center, 2=right. Coordinates are in the field's space
/// (BBox origin 0,0).
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content(
    text: &[u8],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
    widths: &FontWidths,
) -> Vec<u8> {
    let tw = string_width(text, size, widths);
    let tx = quad_offset(q, box_w, tw);
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let escaped = escape_pdf_literal(text);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    write!(out, "/{font} {size:.2} Tf {color} ").unwrap();
    write!(out, "{tx:.2} {ty:.2} Td (").unwrap();
    out.extend_from_slice(&escaped);
    out.extend_from_slice(b") Tj ET Q EMC");
    out
}

/// Single-line appearance content for an embedded (Type0/Identity-H) font.
/// Encodes each char to a 2-byte big-endian GID via `built.gid_for`; chars with
/// no glyph are skipped (matching `drawText`). Horizontal quad offset uses the
/// embedded font's measured advance; vertical baseline matches the WinAnsi
/// single-line builder.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_embedded(
    text: &str,
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
    built: &crate::fonts::BuiltFont,
    font_bytes: &[u8],
) -> Vec<u8> {
    // 2-byte big-endian GID per char with a glyph.
    let mut hex = String::new();
    for ch in text.chars() {
        if let Some(&gid) = built.gid_for.get(&ch) {
            write!(hex, "{gid:04x}").unwrap();
        }
    }
    let tw = crate::fonts::measure_embedded(font_bytes, size, text).unwrap_or(0.0);
    let tx = quad_offset(q, box_w, tw);
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    write!(out, "/{font} {size:.2} Tf {color} ").unwrap();
    write!(out, "{tx:.2} {ty:.2} Td <").unwrap();
    out.extend_from_slice(hex.as_bytes());
    out.extend_from_slice(b"> Tj ET Q EMC");
    out
}

/// Multi-line sibling of `text_appearance_content_embedded`: one hex `Tj` per
/// already-wrapped line (see `fonts::wrap_embedded`), stepping the baseline
/// like `text_appearance_content_multiline`. Propagates a measurement error
/// from `measure_embedded` if the font can't be parsed.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_embedded_multiline(
    lines: &[&str],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    alias: &str,
    built: &crate::fonts::BuiltFont,
    font_bytes: &[u8],
) -> Result<String, String> {
    let leading = size * 1.15;
    let mut out = String::new();
    out.push_str("/Tx BMC q BT ");
    write!(out, "/{alias} {size:.2} Tf {color} ").unwrap();
    write!(out, "{leading:.2} TL ").unwrap();

    let mut ty = box_h - PAD - size;
    for line in lines {
        let tw = crate::fonts::measure_embedded(font_bytes, size, line)?;
        let tx = quad_offset(q, box_w, tw);
        let mut hex = String::new();
        for ch in line.chars() {
            if let Some(&gid) = built.gid_for.get(&ch) {
                write!(hex, "{gid:04x}").unwrap();
            }
        }
        write!(out, "1 0 0 1 {tx:.2} {ty:.2} Tm <{hex}> Tj ").unwrap();
        ty -= leading;
    }
    out.push_str("ET Q EMC");
    Ok(out)
}

/// Build the content stream for a wrapped, multi-line text appearance. `lines`
/// are pre-wrapped WinAnsi byte strings (see `wrap_lines`). `q` is the quadding
/// applied per line: 0=left, 1=center, 2=right. Text is top-aligned: the first
/// baseline sits near the top of the box (Acrobat-like), and successive lines
/// step down by the leading (`size * 1.15`). Coordinates are in the field's
/// space (BBox origin 0,0). Each line uses an absolute `Tm` so its horizontal
/// quad offset and vertical baseline are independent of the previous line.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_multiline(
    lines: &[Vec<u8>],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
    widths: &FontWidths,
) -> Vec<u8> {
    let leading = size * 1.15;
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    write!(out, "/{font} {size:.2} Tf {color} ").unwrap();
    write!(out, "{leading:.2} TL ").unwrap();

    // First baseline near the top of the box; step down by the leading per line.
    let mut ty = box_h - PAD - size;
    for line in lines {
        let tw = string_width(line, size, widths);
        let tx = quad_offset(q, box_w, tw);
        let escaped = escape_pdf_literal(line);
        // Absolute text matrix per line keeps each line's quad offset and
        // baseline independent of the running text matrix.
        write!(out, "1 0 0 1 {tx:.2} {ty:.2} Tm (").unwrap();
        out.extend_from_slice(&escaped);
        out.extend_from_slice(b") Tj ");
        ty -= leading;
    }
    out.extend_from_slice(b"ET Q EMC");
    out
}

/// Build an empty appearance content stream (a well-formed but value-free
/// marked-content block). Used for password fields, whose value must never be
/// rendered into the appearance.
pub fn text_appearance_content_empty() -> Vec<u8> {
    b"/Tx BMC q BT ET Q EMC".to_vec()
}

/// Build the content stream for a comb text field: a single line split into
/// `max_len` equal cells, with character `i` centered in cell `i`. Mirrors the
/// PDF Comb flag layout (fixed pitch regardless of the value). `text` is a
/// pre-encoded WinAnsi byte string; only the first `max_len` bytes are placed.
#[allow(clippy::too_many_arguments)]
pub fn text_appearance_content_comb(
    text: &[u8],
    size: f32,
    box_w: f32,
    box_h: f32,
    max_len: i64,
    color: &str,
    font: &str,
    widths: &FontWidths,
) -> Vec<u8> {
    let cells = max_len.max(1) as f32;
    let cell_w = box_w / cells;
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    write!(out, "/{font} {size:.2} Tf {color} ").unwrap();
    for (i, &b) in text.iter().take(max_len.max(0) as usize).enumerate() {
        let cw = string_width(&[b], size, widths);
        let cx = cell_w * (i as f32 + 0.5);
        let tx = (cx - cw / 2.0).max(0.0);
        let escaped = escape_pdf_literal(&[b]);
        // Absolute text matrix per glyph centers it in its cell, independent of
        // the running text position.
        write!(out, "1 0 0 1 {tx:.2} {ty:.2} Tm (").unwrap();
        out.extend_from_slice(&escaped);
        out.extend_from_slice(b") Tj ");
    }
    out.extend_from_slice(b"ET Q EMC");
    out
}

/// Build a Form XObject appearance stream of size `box_w`x`box_h` whose
/// Resources reference the font named `font` at indirect object `font_ref`.
pub fn build_appearance_xobject(
    content: Vec<u8>,
    box_w: f32,
    box_h: f32,
    font: &str,
    font_ref: lopdf::ObjectId,
) -> Stream {
    let mut font_dict = Dictionary::new();
    font_dict.set(font.as_bytes().to_vec(), Object::Reference(font_ref));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_dict));

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("FormType", Object::Integer(1));
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(box_w),
            Object::Real(box_h),
        ]),
    );
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, content)
}

#[derive(Debug)]
pub struct JpegInfo {
    pub width: i64,
    pub height: i64,
    pub color_space: &'static str,
}

#[derive(Clone, Debug)]
pub enum SignatureImage {
    Jpeg {
        data: Vec<u8>,
        info: ImageInfo,
    },
    Raw {
        data: Vec<u8>,
        info: ImageInfo,
        /// Per-pixel alpha plane (one byte/pixel) for PNG color types 4/6.
        /// `None` for fully opaque images (color types 0/2).
        alpha: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub width: i64,
    pub height: i64,
    pub color_space: &'static str,
}

impl SignatureImage {
    pub fn info(&self) -> &ImageInfo {
        match self {
            SignatureImage::Jpeg { info, .. } | SignatureImage::Raw { info, .. } => info,
        }
    }
}

pub fn signature_image(data: &[u8]) -> Result<SignatureImage, String> {
    if data.starts_with(&[0xff, 0xd8]) {
        let info = jpeg_info(data)?;
        return Ok(SignatureImage::Jpeg {
            data: data.to_vec(),
            info: ImageInfo {
                width: info.width,
                height: info.height,
                color_space: info.color_space,
            },
        });
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_image(data);
    }
    Err("signature image must be a JPEG or supported PNG".to_string())
}

/// Read dimensions from a JPEG SOF segment. This deliberately only validates
/// enough to embed visual signatures as `/DCTDecode` image XObjects.
pub fn jpeg_info(data: &[u8]) -> Result<JpegInfo, String> {
    if data.len() < 4 || data[0] != 0xff || data[1] != 0xd8 {
        return Err("signature image must be a JPEG".to_string());
    }

    let mut i = 2usize;
    while i + 3 < data.len() {
        while i < data.len() && data[i] == 0xff {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        let marker = data[i];
        i += 1;

        if marker == 0xd9 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if i + 2 > data.len() {
            break;
        }
        let len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
        if len < 2 || i + len > data.len() {
            break;
        }

        if is_sof_marker(marker) {
            if len < 8 {
                break;
            }
            let height = u16::from_be_bytes([data[i + 3], data[i + 4]]) as i64;
            let width = u16::from_be_bytes([data[i + 5], data[i + 6]]) as i64;
            let components = data[i + 7];
            let color_space = match components {
                1 => "DeviceGray",
                3 => "DeviceRGB",
                n => {
                    return Err(format!(
                        "unsupported JPEG with {n} color components (CMYK JPEGs are not supported)"
                    ));
                }
            };
            if width > 0 && height > 0 {
                return Ok(JpegInfo {
                    width,
                    height,
                    color_space,
                });
            }
            break;
        }

        i += len;
    }

    Err("could not read JPEG dimensions".to_string())
}

fn is_sof_marker(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

/// Dictionary common to every image XObject: an 8-bit `/Image` of the given
/// dimensions, color space, and decode filter (e.g. `DCTDecode`, `FlateDecode`).
fn image_xobject_dict(width: i64, height: i64, color_space: &str, filter: &[u8]) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(width));
    dict.set("Height", Object::Integer(height));
    dict.set("ColorSpace", Object::Name(color_space.as_bytes().to_vec()));
    dict.set("BitsPerComponent", Object::Integer(8));
    dict.set("Filter", Object::Name(filter.to_vec()));
    dict
}

pub fn build_jpeg_image_xobject(data: Vec<u8>, info: &JpegInfo) -> Stream {
    let dict = image_xobject_dict(info.width, info.height, info.color_space, b"DCTDecode");
    Stream::new(dict, data)
}

pub fn build_signature_image_xobject(image: SignatureImage) -> Stream {
    match image {
        SignatureImage::Jpeg { data, info } => build_jpeg_image_xobject(
            data,
            &JpegInfo {
                width: info.width,
                height: info.height,
                color_space: info.color_space,
            },
        ),
        SignatureImage::Raw { data, info, .. } => build_raw_image_xobject(data, &info),
    }
}

/// Build the image XObject(s) for `image`, registering each created stream via
/// `add` and returning the object id of the main color image. PNGs carrying an
/// alpha channel get a DeviceGray `/SMask` soft-mask image referenced by the
/// main image; opaque images and JPEGs get no soft mask.
pub fn build_image_xobjects(
    image: SignatureImage,
    add: &mut dyn FnMut(Object) -> lopdf::ObjectId,
) -> lopdf::ObjectId {
    match image {
        SignatureImage::Jpeg { data, info } => {
            let stream = build_jpeg_image_xobject(
                data,
                &JpegInfo {
                    width: info.width,
                    height: info.height,
                    color_space: info.color_space,
                },
            );
            add(Object::Stream(stream))
        }
        SignatureImage::Raw {
            data,
            info,
            alpha: None,
        } => add(Object::Stream(build_raw_image_xobject(data, &info))),
        SignatureImage::Raw {
            data,
            info,
            alpha: Some(a),
        } => {
            let smask_stream = build_raw_image_xobject(
                a,
                &ImageInfo {
                    width: info.width,
                    height: info.height,
                    color_space: "DeviceGray",
                },
            );
            let smask_id = add(Object::Stream(smask_stream));
            let mut main = build_raw_image_xobject(data, &info);
            main.dict.set("SMask", Object::Reference(smask_id));
            add(Object::Stream(main))
        }
    }
}

fn build_raw_image_xobject(data: Vec<u8>, info: &ImageInfo) -> Stream {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).expect("Vec writes cannot fail");
    let compressed = encoder.finish().expect("zlib finish cannot fail for Vec");

    let dict = image_xobject_dict(info.width, info.height, info.color_space, b"FlateDecode");
    Stream::new(dict, compressed).with_compression(false)
}

/// Decode a color-type-3 (indexed/palette) PNG into RGB (+ optional alpha).
///
/// Each pixel in the raw scanline data is a palette index (one byte, 8-bit).
/// After unfiltering, each index is expanded to an RGB triple via `palette`.
/// If `trns` is non-empty, a per-pixel alpha plane is also built:
/// `trns[index]` if within range, 255 (opaque) otherwise.
fn png_image_indexed(
    width: usize,
    height: usize,
    idat: &[u8],
    palette: &[(u8, u8, u8)],
    trns: &[u8],
) -> Result<SignatureImage, String> {
    let mut decoder = ZlibDecoder::new(idat);
    let mut inflated = Vec::new();
    decoder
        .read_to_end(&mut inflated)
        .map_err(|e| e.to_string())?;

    // Indexed row: 1 byte per pixel (the palette index).
    let stride = width;
    let expected = height
        .checked_mul(stride + 1)
        .ok_or_else(|| "PNG image is too large".to_string())?;
    if inflated.len() < expected {
        return Err("truncated PNG image data".to_string());
    }

    let has_trns = !trns.is_empty();
    let mut alpha: Option<Vec<u8>> = if has_trns {
        Some(Vec::with_capacity(width * height))
    } else {
        None
    };

    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    let mut out = Vec::with_capacity(width * height * 3);
    let mut offset = 0usize;

    for _ in 0..height {
        let filter = inflated[offset];
        offset += 1;
        cur.copy_from_slice(&inflated[offset..offset + stride]);
        offset += stride;
        // bpp = 1 for indexed (one byte per pixel index).
        unfilter_png_row(filter, &mut cur, &prev, 1)?;

        for &idx in &cur {
            let idx_usize = idx as usize;
            if idx_usize >= palette.len() {
                return Err("PNG palette index out of range".to_string());
            }
            let (r, g, b) = palette[idx_usize];
            out.push(r);
            out.push(g);
            out.push(b);
            if let Some(a) = alpha.as_mut() {
                let av = trns.get(idx_usize).copied().unwrap_or(255);
                a.push(av);
            }
        }

        std::mem::swap(&mut prev, &mut cur);
    }

    Ok(SignatureImage::Raw {
        data: out,
        info: ImageInfo {
            width: width as i64,
            height: height as i64,
            color_space: "DeviceRGB",
        },
        alpha,
    })
}

fn png_image(data: &[u8]) -> Result<SignatureImage, String> {
    let mut pos = 8usize;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut idat = Vec::new();
    // Color type 3 (indexed): palette RGB triples and optional per-index alpha.
    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut trns: Vec<u8> = Vec::new(); // tRNS alpha values (one per palette index)

    while pos + 12 <= data.len() {
        let len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + 4 + len + 4 > data.len() {
            return Err("truncated PNG chunk".to_string());
        }
        let kind = &data[pos..pos + 4];
        pos += 4;
        let chunk = &data[pos..pos + len];
        pos += len;
        pos += 4; // CRC; decoding is structural only for v1.

        match kind {
            b"IHDR" => {
                if chunk.len() != 13 {
                    return Err("invalid PNG IHDR".to_string());
                }
                width = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize;
                height = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as usize;
                bit_depth = chunk[8];
                color_type = chunk[9];
                interlace = chunk[12];
            }
            b"PLTE" => {
                if !chunk.len().is_multiple_of(3) {
                    return Err("invalid PNG PLTE chunk length".to_string());
                }
                palette = chunk.chunks_exact(3).map(|t| (t[0], t[1], t[2])).collect();
            }
            b"tRNS" => {
                trns = chunk.to_vec();
            }
            b"IDAT" => idat.extend_from_slice(chunk),
            b"IEND" => break,
            _ => {}
        }
    }

    if width == 0 || height == 0 {
        return Err("invalid PNG dimensions".to_string());
    }
    if interlace != 0 {
        return Err("interlaced PNG signatures are not supported".to_string());
    }

    // Color type 3 (indexed/palette) supports bit depths 1, 2, 4, 8.
    // We support 8-bit indices only; sub-byte packing is not implemented.
    if color_type == 3 {
        if bit_depth != 8 {
            return Err(
                "only 8-bit indexed PNG signatures are supported (bit depth must be 8)".to_string(),
            );
        }
        if palette.is_empty() {
            return Err("PNG color type 3 requires a PLTE chunk".to_string());
        }
        return png_image_indexed(width, height, &idat, &palette, &trns);
    }

    if bit_depth != 8 {
        return Err("only 8-bit PNG signatures are supported".to_string());
    }

    let (src_components, out_components, color_space) = match color_type {
        0 => (1usize, 1usize, "DeviceGray"),
        2 => (3usize, 3usize, "DeviceRGB"),
        4 => (2usize, 1usize, "DeviceGray"),
        6 => (4usize, 3usize, "DeviceRGB"),
        _ => return Err("unsupported PNG color type for signatures".to_string()),
    };

    let mut decoder = ZlibDecoder::new(idat.as_slice());
    let mut inflated = Vec::new();
    decoder
        .read_to_end(&mut inflated)
        .map_err(|e| e.to_string())?;

    let stride = width
        .checked_mul(src_components)
        .ok_or_else(|| "PNG row is too wide".to_string())?;
    let expected = height
        .checked_mul(stride + 1)
        .ok_or_else(|| "PNG image is too large".to_string())?;
    if inflated.len() < expected {
        return Err("truncated PNG image data".to_string());
    }

    // Color types 4 (gray+alpha) and 6 (RGBA) carry a trailing alpha byte per
    // pixel that the color `out` strips; collect it into a separate plane so the
    // caller can emit a DeviceGray /SMask.
    let has_alpha = src_components > out_components;
    let mut alpha: Option<Vec<u8>> = if has_alpha {
        Some(Vec::with_capacity(width * height))
    } else {
        None
    };

    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    let mut out = Vec::with_capacity(width * height * out_components);
    let mut offset = 0usize;
    for _ in 0..height {
        let filter = inflated[offset];
        offset += 1;
        cur.copy_from_slice(&inflated[offset..offset + stride]);
        offset += stride;
        unfilter_png_row(filter, &mut cur, &prev, src_components)?;
        push_png_output_row(&cur, src_components, out_components, &mut out);
        if let Some(a) = alpha.as_mut() {
            for px in cur.chunks_exact(src_components) {
                a.push(px[src_components - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    Ok(SignatureImage::Raw {
        data: out,
        info: ImageInfo {
            width: width as i64,
            height: height as i64,
            color_space,
        },
        alpha,
    })
}

fn unfilter_png_row(filter: u8, row: &mut [u8], prev: &[u8], bpp: usize) -> Result<(), String> {
    match filter {
        0 => {}
        1 => {
            for i in 0..row.len() {
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                row[i] = row[i].wrapping_add(left);
            }
        }
        2 => {
            for i in 0..row.len() {
                row[i] = row[i].wrapping_add(prev[i]);
            }
        }
        3 => {
            for i in 0..row.len() {
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                let up = prev[i];
                row[i] = row[i].wrapping_add(((left as u16 + up as u16) / 2) as u8);
            }
        }
        4 => {
            for i in 0..row.len() {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                row[i] = row[i].wrapping_add(paeth(a, b, c));
            }
        }
        _ => return Err("unsupported PNG row filter".to_string()),
    }
    Ok(())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn push_png_output_row(
    row: &[u8],
    src_components: usize,
    out_components: usize,
    out: &mut Vec<u8>,
) {
    for px in row.chunks_exact(src_components) {
        out.extend_from_slice(&px[..out_components]);
    }
}

pub fn build_signature_appearance_xobject(
    image_ref: lopdf::ObjectId,
    image_w: f32,
    image_h: f32,
    box_w: f32,
    box_h: f32,
) -> Stream {
    let scale = (box_w / image_w).max(box_h / image_h);
    let draw_w = image_w * scale;
    let draw_h = image_h * scale;
    let tx = (box_w - draw_w) / 2.0;
    let ty = (box_h - draw_h) / 2.0;
    let content =
        format!("q {draw_w:.2} 0 0 {draw_h:.2} {tx:.2} {ty:.2} cm /SigImg Do Q").into_bytes();

    let mut xobjects = Dictionary::new();
    xobjects.set("SigImg", Object::Reference(image_ref));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("FormType", Object::Integer(1));
    dict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(box_w),
            Object::Real(box_h),
        ]),
    );
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, content)
}

/// Build the content stream for a multi-select list box appearance: one row per
/// option from top to bottom; each selected row gets a filled highlight
/// rectangle drawn behind its text. `selected[i]` toggles option `i`.
#[allow(clippy::too_many_arguments)]
pub fn listbox_multi_content(
    options: &[Vec<u8>],
    selected: &[bool],
    da_size: f32,
    box_w: f32,
    box_h: f32,
    color: &str,
    font: &str,
) -> Vec<u8> {
    // Row height: honor a positive DA size, else a sane default capped by box.
    let line = if da_size > 0.0 { da_size } else { MAX_AUTO };
    let row_h = (line + 2.0).max(MIN_AUTO + 2.0);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q ");

    // 1) Highlight rectangles for selected rows (painted first, behind text).
    for (i, &sel) in selected.iter().enumerate() {
        if !sel {
            continue;
        }
        // Top-aligned: row 0 sits just under the top edge.
        let y = box_h - row_h * (i as f32 + 1.0);
        write!(
            out,
            "0.60 0.75 0.85 rg {:.2} {:.2} {:.2} {:.2} re f ",
            PAD,
            y,
            (box_w - 2.0 * PAD).max(0.0),
            row_h
        )
        .unwrap();
    }

    // 2) Text for every option, top to bottom.
    out.extend_from_slice(b"BT ");
    write!(out, "/{font} {line:.2} Tf {color} ").unwrap();
    for (i, opt) in options.iter().enumerate() {
        let baseline = box_h - row_h * (i as f32) - line;
        let escaped = escape_pdf_literal(opt);
        write!(out, "1 0 0 1 {PAD:.2} {baseline:.2} Tm (").unwrap();
        out.extend_from_slice(&escaped);
        out.extend_from_slice(b") Tj ");
    }
    out.extend_from_slice(b"ET Q EMC");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listbox_multi_highlights_selected_rows() {
        let options = vec![b"ES".to_vec(), b"EN".to_vec(), b"PT".to_vec()];
        let selected = vec![true, false, true];
        let content = listbox_multi_content(&options, &selected, 0.0, 100.0, 60.0, "0 g", "Helv");
        let s = String::from_utf8_lossy(&content);
        // Marked content + save/restore framing.
        assert!(s.starts_with("/Tx BMC q"));
        assert!(s.trim_end().ends_with("EMC"));
        // Every option is drawn.
        assert!(s.contains("(ES) Tj"), "got: {s}");
        assert!(s.contains("(EN) Tj"), "got: {s}");
        assert!(s.contains("(PT) Tj"), "got: {s}");
        // Two selected rows -> two highlight rectangles filled with the blue rg.
        let blue = "0.60 0.75 0.85 rg";
        assert_eq!(s.matches(blue).count(), 2, "expected 2 highlights in: {s}");
        assert_eq!(s.matches(" re").count(), 2, "expected 2 rectangles in: {s}");
    }

    #[test]
    fn measures_helvetica_width() {
        let w = measure_text_width("Helvetica", 12.0, "Hello").unwrap();
        assert!(w > 20.0 && w < 40.0, "width was {w}");
    }

    #[test]
    fn wrap_lines_preserves_hard_breaks() {
        // Wide box so no soft wrapping happens; only the explicit \n splits.
        let lines = wrap_lines(b"alpha\nbeta", 10.0, 1000.0, &helvetica_widths());
        assert_eq!(lines, vec![b"alpha".to_vec(), b"beta".to_vec()]);
    }

    #[test]
    fn wrap_lines_normalizes_crlf() {
        let lines = wrap_lines(b"alpha\r\nbeta\rgamma", 10.0, 1000.0, &helvetica_widths());
        assert_eq!(
            lines,
            vec![b"alpha".to_vec(), b"beta".to_vec(), b"gamma".to_vec()]
        );
    }

    // wrap_str: width = byte length (each char counts 1) makes assertions exact.
    fn char_len(s: &str) -> f32 {
        s.chars().count() as f32
    }

    #[test]
    fn wrap_str_greedy_wraps_on_spaces() {
        assert_eq!(wrap_str("aaa bbb ccc", 7.0, char_len), "aaa bbb\nccc");
    }

    #[test]
    fn wrap_str_keeps_hard_breaks() {
        assert_eq!(wrap_str("aa\nbb cc", 3.0, char_len), "aa\nbb\ncc");
    }

    #[test]
    fn wrap_str_overlong_word_gets_own_line() {
        assert_eq!(wrap_str("aaaaaa bb", 3.0, char_len), "aaaaaa\nbb");
    }

    #[test]
    fn wrap_str_fits_on_one_line() {
        assert_eq!(wrap_str("hi there", 100.0, char_len), "hi there");
    }

    #[test]
    fn wrap_str_collapses_space_runs() {
        assert_eq!(wrap_str("aa   bb", 100.0, char_len), "aa bb");
    }

    #[test]
    fn wrap_str_normalizes_crlf() {
        // The drift this fixes: the old TS wrapText only split on "\n".
        assert_eq!(wrap_str("a\r\nb\rc", 100.0, char_len), "a\nb\nc");
    }

    #[test]
    fn wrap_standard14_wraps_long_line() {
        let out = wrap_standard14("the quick brown fox jumps", "Helvetica", 12.0, 80.0);
        assert!(out.contains('\n'), "expected wrapping, got {out:?}");
    }

    #[test]
    fn wrap_lines_greedy_word_wrap() {
        // "aaaa" at size 10 is 4 * 0.556 * 10 = 22.24pt wide; a ~30pt box fits one
        // word per line (two words + a space exceed 30pt), forcing a wrap.
        let lines = wrap_lines(b"aaaa aaaa", 10.0, 30.0, &helvetica_widths());
        assert_eq!(lines, vec![b"aaaa".to_vec(), b"aaaa".to_vec()]);
    }

    #[test]
    fn wrap_lines_long_word_overflows_on_own_line() {
        // A single word wider than avail_w gets its own line; the short word wraps after.
        let lines = wrap_lines(b"wwwwwwwwww hi", 10.0, 30.0, &helvetica_widths());
        assert_eq!(lines, vec![b"wwwwwwwwww".to_vec(), b"hi".to_vec()]);
    }

    #[test]
    fn wrap_lines_empty_string_is_single_empty_line() {
        assert_eq!(
            wrap_lines(b"", 10.0, 100.0, &helvetica_widths()),
            vec![Vec::<u8>::new()]
        );
    }

    #[test]
    fn wrap_lines_blank_paragraph_preserved() {
        let lines = wrap_lines(b"a\n\nb", 10.0, 1000.0, &helvetica_widths());
        assert_eq!(lines, vec![b"a".to_vec(), Vec::<u8>::new(), b"b".to_vec()]);
    }

    #[test]
    fn multiline_content_emits_multiple_tj_and_tl() {
        let lines = vec![b"hello".to_vec(), b"world".to_vec()];
        let c = text_appearance_content_multiline(
            &lines,
            10.0,
            100.0,
            40.0,
            0,
            "0 g",
            "Helv",
            &helvetica_widths(),
        );
        let s = String::from_utf8(c).unwrap();
        assert!(s.contains("/Tx BMC"));
        assert!(s.contains("/Helv 10.00 Tf"));
        assert!(s.contains("TL"), "missing leading operator: {s}");
        assert_eq!(s.matches(" Tj").count(), 2, "expected two Tj: {s}");
        assert!(s.contains("(hello) Tj"));
        assert!(s.contains("(world) Tj"));
        assert!(s.ends_with("ET Q EMC"));
    }

    #[test]
    fn multiline_content_escapes_text() {
        let lines = vec![b"a(b)".to_vec()];
        let c = text_appearance_content_multiline(
            &lines,
            10.0,
            100.0,
            40.0,
            0,
            "0 g",
            "Helv",
            &helvetica_widths(),
        );
        assert!(String::from_utf8(c).unwrap().contains("(a\\(b\\)) Tj"));
    }

    #[test]
    fn multiline_content_quads_right() {
        // Right-quad: each line's tx = box_w - PAD - line_width, so a short line
        // sits well to the right (tx well above the left PAD of 2.0).
        let lines = vec![b"hi".to_vec()];
        let c = text_appearance_content_multiline(
            &lines,
            10.0,
            200.0,
            40.0,
            2,
            "0 g",
            "Helv",
            &helvetica_widths(),
        );
        let s = String::from_utf8(c).unwrap();
        // "hi" width = (222 + 556)/1000 * 10 = 7.78; tx = 200 - 2 - 7.78 = 190.22.
        assert!(
            s.contains("190.22"),
            "expected right-quad tx 190.22 in: {s}"
        );
    }

    #[test]
    fn measure_scales_linearly_with_size() {
        let a = measure_text_width("Helvetica", 10.0, "ABCDEF").unwrap();
        let b = measure_text_width("Helvetica", 20.0, "ABCDEF").unwrap();
        assert!((b - 2.0 * a).abs() < 0.01);
    }

    #[test]
    fn measure_empty_is_zero() {
        assert_eq!(measure_text_width("Helvetica", 12.0, "").unwrap(), 0.0);
    }

    #[test]
    fn measure_unknown_font_errors() {
        assert!(
            measure_text_width("Comic Sans", 12.0, "x")
                .unwrap_err()
                .contains("font")
        );
    }

    #[test]
    fn encodes_spanish_to_winansi() {
        // á=0xE1, í=0xED, ñ=0xF1
        assert_eq!(
            encode_winansi("García"),
            vec![b'G', b'a', b'r', b'c', 0xED, b'a']
        );
        assert_eq!(encode_winansi("ñ"), vec![0xF1]);
    }

    #[test]
    fn escapes_pdf_literal_specials() {
        assert_eq!(escape_pdf_literal(b"a(b)\\c"), b"a\\(b\\)\\\\c".to_vec());
    }

    #[test]
    fn helvetica_widths_match_afm() {
        assert_eq!(helvetica_widths().width(b' '), 278);
        assert_eq!(helvetica_widths().width(b'A'), 667);
        assert_eq!(helvetica_widths().width(b'i'), 222);
        assert_eq!(helvetica_widths().width(b'W'), 944);
    }

    #[test]
    fn string_width_scales_with_size() {
        // "AA" at size 10 = 2 * 667/1000 * 10 = 13.34
        let w = string_width(b"AA", 10.0, &helvetica_widths());
        assert!((w - 13.34).abs() < 0.01, "got {w}");
    }

    #[test]
    fn parses_da() {
        let da = parse_da("/Helv 0 Tf 0 g");
        assert_eq!(da.font, "Helv");
        assert_eq!(da.size, 0.0);
        assert_eq!(da.color, "0 g");
    }

    #[test]
    fn auto_size_uses_height_then_shrinks_to_width() {
        // Tall, wide box, short text → height-capped at 12.
        assert!((auto_size(0.0, b"AB", 300.0, 14.0, &helvetica_widths()) - 12.0).abs() < 0.01);
        // Narrow box forces shrink below the height cap.
        let s = auto_size(0.0, b"WWWWWWWWWW", 30.0, 14.0, &helvetica_widths());
        assert!((4.0..12.0).contains(&s), "got {s}");
        // Explicit DA size is honored as-is.
        assert_eq!(auto_size(9.0, b"x", 300.0, 50.0, &helvetica_widths()), 9.0);
    }

    #[test]
    fn content_has_text_operators() {
        let c = text_appearance_content(
            b"Hi",
            10.0,
            100.0,
            14.0,
            0,
            "0 g",
            "Helv",
            &helvetica_widths(),
        );
        let s = String::from_utf8(c).unwrap();
        assert!(s.contains("/Tx BMC"));
        assert!(s.contains("/Helv 10.00 Tf"));
        assert!(s.contains("(Hi) Tj"));
        assert!(s.contains("ET Q EMC"));
    }

    #[test]
    fn content_escapes_text() {
        let c = text_appearance_content(
            b"a(b)",
            10.0,
            100.0,
            14.0,
            0,
            "0 g",
            "Helv",
            &helvetica_widths(),
        );
        assert!(String::from_utf8(c).unwrap().contains("(a\\(b\\)) Tj"));
    }

    #[test]
    fn standard_14_tables_match_known_afm_values() {
        let helv = helvetica_widths();
        assert_eq!(helv.width(b'A'), 667);
        assert_eq!(helv.width(b' '), 278);
        let times = standard_14_widths("Times-Roman").unwrap();
        assert_eq!(times.width(b'A'), 722);
        assert_eq!(times.width(b' '), 250);
        let courier = standard_14_widths("Courier").unwrap();
        assert_eq!(courier.width(b'A'), 600);
        assert_eq!(courier.width(b'W'), 600);
    }

    #[test]
    fn maps_da_base_fonts_to_standard_14_tables() {
        // Subset prefixes and the common TrueType aliases resolve.
        let bold = standard_14_widths("ABCDEF+Arial-BoldMT").unwrap();
        assert_eq!(bold.width(b'A'), 722); // Helvetica-Bold 'A'
        assert!(standard_14_widths("TimesNewRomanPS-ItalicMT").is_some());
        assert!(standard_14_widths("CourierNewPSMT").is_some());
        assert!(standard_14_widths("Wingdings").is_none());
    }

    #[test]
    fn encodes_winansi_beyond_latin1() {
        assert_eq!(encode_winansi("€"), vec![0x80]);
        assert_eq!(encode_winansi("\u{201C}"), vec![0x93]); // left double quote
        assert_eq!(encode_winansi("漢"), vec![b'?']);
    }

    #[test]
    fn reads_jpeg_dimensions() {
        let jpg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00,
            0x02, 0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
        ];
        let info = jpeg_info(&jpg).unwrap();
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 2);
        assert_eq!(info.color_space, "DeviceRGB");
    }

    #[test]
    fn rejects_cmyk_jpeg() {
        let mut jpg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00,
            0x02, 0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
        ];
        jpg[17] = 4; // SOF0 component count -> CMYK
        let err = jpeg_info(&jpg).unwrap_err();
        assert!(err.contains("components"), "got: {err}");
    }

    #[test]
    fn signature_appearance_covers_the_field_box() {
        let s = build_signature_appearance_xobject((99, 0), 1254.0, 741.0, 500.0, 25.0);
        let content = String::from_utf8(s.content).unwrap();
        assert!(
            content.starts_with("q 500.00 0 0 295.45 "),
            "got: {content}"
        );
        assert!(content.contains(" cm /SigImg Do Q"), "got: {content}");
    }

    #[test]
    fn reads_png_signature_image() {
        let img = signature_image(tiny_rgba_png()).unwrap();
        let info = img.info();
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
        assert_eq!(info.color_space, "DeviceRGB");
        match img {
            SignatureImage::Raw { data, .. } => assert_eq!(data, vec![255, 0, 0]),
            SignatureImage::Jpeg { .. } => panic!("expected raw PNG image"),
        }
    }

    #[test]
    fn rejects_non_image_signature_data() {
        let err = signature_image(b"not an image").unwrap_err();
        assert!(err.contains("JPEG or supported PNG"), "got: {err}");
    }

    #[test]
    fn rgba_png_extracts_alpha_plane() {
        let img = signature_image(tiny_rgba_png()).unwrap();
        match img {
            SignatureImage::Raw {
                ref info,
                ref alpha,
                ref data,
            } => {
                assert_eq!(info.color_space, "DeviceRGB");
                let px = (info.width * info.height) as usize;
                assert_eq!(
                    data.len(),
                    px * 3,
                    "color data must be RGB (alpha stripped)"
                );
                let a = alpha.as_ref().expect("RGBA png must yield an alpha plane");
                assert_eq!(a.len(), px, "alpha plane must be one byte per pixel");
            }
            _ => panic!("expected Raw"),
        }
    }

    #[test]
    fn opaque_png_has_no_alpha() {
        let img = signature_image(tiny_rgb_png()).unwrap();
        if let SignatureImage::Raw { alpha, .. } = img {
            assert!(alpha.is_none(), "RGB png must not produce an alpha plane");
        } else {
            panic!("expected Raw");
        }
    }

    #[test]
    fn build_image_xobjects_sets_smask_for_alpha() {
        use lopdf::{Document, Object};
        let mut doc = Document::with_version("1.7");
        let img = signature_image(tiny_rgba_png()).unwrap();
        let mut add = |o: Object| doc.add_object(o);
        let main_id = build_image_xobjects(img, &mut add);
        let main = doc.get_object(main_id).unwrap().as_stream().unwrap();
        let smask_ref = main
            .dict
            .get(b"SMask")
            .expect("main image must have /SMask")
            .as_reference()
            .unwrap();
        let smask = doc.get_object(smask_ref).unwrap().as_stream().unwrap();
        assert_eq!(
            smask.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        assert_eq!(
            smask.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Image"
        );
    }

    #[test]
    fn build_image_xobjects_no_smask_for_opaque() {
        use lopdf::{Document, Object};
        let mut doc = Document::with_version("1.7");
        let img = signature_image(tiny_rgb_png()).unwrap();
        let mut add = |o: Object| doc.add_object(o);
        let main_id = build_image_xobjects(img, &mut add);
        let main = doc.get_object(main_id).unwrap().as_stream().unwrap();
        assert!(
            main.dict.get(b"SMask").is_err(),
            "opaque image must not have /SMask"
        );
    }

    #[test]
    fn decodes_palette_png() {
        let img = signature_image(tiny_palette_png()).unwrap();
        match img {
            SignatureImage::Raw { info, data, .. } => {
                assert_eq!(info.color_space, "DeviceRGB");
                assert_eq!(data.len(), (info.width * info.height) as usize * 3);
            }
            _ => panic!("expected Raw"),
        }
    }

    #[test]
    fn palette_png_expands_correct_rgb() {
        // tiny_palette_png() has one palette entry: (255, 0, 0) at index 0,
        // and a 1x1 image with pixel index 0. Decoded RGB must be [255, 0, 0].
        let img = signature_image(tiny_palette_png()).unwrap();
        if let SignatureImage::Raw { data, alpha, .. } = img {
            assert_eq!(data, vec![255, 0, 0]);
            assert!(
                alpha.is_none(),
                "opaque palette PNG must not have alpha plane"
            );
        } else {
            panic!("expected Raw");
        }
    }

    #[test]
    fn palette_png_with_trns_has_alpha() {
        // tiny_palette_png_with_trns() has one palette entry (255, 0, 0) with
        // tRNS alpha = 128 for index 0. Decoded must yield alpha = [128].
        let img = signature_image(tiny_palette_png_with_trns()).unwrap();
        if let SignatureImage::Raw { data, alpha, info } = img {
            assert_eq!(info.color_space, "DeviceRGB");
            assert_eq!(data, vec![255, 0, 0]);
            let a = alpha.expect("palette PNG with tRNS must yield alpha plane");
            assert_eq!(a, vec![128]);
        } else {
            panic!("expected Raw");
        }
    }

    /// Minimal 1×1 indexed (color type 3) PNG with one palette entry: red (255,0,0).
    /// Hand-built with correct CRC32 values. No tRNS → alpha: None.
    fn tiny_palette_png() -> &'static [u8] {
        &[
            // PNG signature
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // IHDR: length=13
            0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // "IHDR"
            0x00, 0x00, 0x00, 0x01, // width=1
            0x00, 0x00, 0x00, 0x01, // height=1
            0x08, // bit_depth=8
            0x03, // color_type=3 (indexed)
            0x00, 0x00, 0x00, // compression, filter, interlace
            0x28, 0xcb, 0x34, 0xbb, // CRC
            // PLTE: length=3 (one RGB entry: red)
            0x00, 0x00, 0x00, 0x03, 0x50, 0x4c, 0x54, 0x45, // "PLTE"
            0xff, 0x00, 0x00, // (255, 0, 0)
            0x19, 0xe2, 0x09, 0x37, // CRC
            // IDAT: zlib of [filter=0, index=0]
            0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, // "IDAT"
            0x78, 0xda, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde,
            0xfc, // CRC
            // IEND
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, // "IEND"
            0xae, 0x42, 0x60, 0x82, // CRC
        ]
    }

    /// Minimal 1×1 indexed PNG with tRNS: palette entry 0 = red, alpha = 128.
    fn tiny_palette_png_with_trns() -> &'static [u8] {
        &[
            // PNG signature
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // IHDR
            0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x08, 0x03, 0x00, 0x00, 0x00, 0x28, 0xcb, 0x34, 0xbb, // PLTE
            0x00, 0x00, 0x00, 0x03, 0x50, 0x4c, 0x54, 0x45, 0xff, 0x00, 0x00, 0x19, 0xe2, 0x09,
            0x37, // tRNS: alpha=128 for index 0
            0x00, 0x00, 0x00, 0x01, 0x74, 0x52, 0x4e, 0x53, // "tRNS"
            0x80, // alpha=128
            0xad, 0x5e, 0x5b, 0x46, // CRC
            // IDAT
            0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde, 0xfc, // IEND
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ]
    }

    fn tiny_rgb_png() -> &'static [u8] {
        &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92,
            0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ]
    }

    fn tiny_rgba_png() -> &'static [u8] {
        &[
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
            0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
            0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }
}

// TDD-red spec for embedded (Type0) form-field appearance content. Targets a
// new function mirroring the WinAnsi single-line `text_appearance_content`
// builder but emitting Identity-H 2-byte GID show strings using a `BuiltFont`'s
// char->gid map. Fails to compile until that function exists. (The create path
// renders text fields single-line, so no multiline embedded builder is needed.)
#[cfg(test)]
mod embedded_field_appearance_tests {
    use super::text_appearance_content_embedded;
    use crate::fonts::BuiltFont;
    use std::collections::HashMap;

    const FONT: &[u8] = include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

    fn built(map: &[(char, u16)]) -> BuiltFont {
        BuiltFont {
            gid_for: map.iter().copied().collect::<HashMap<char, u16>>(),
        }
    }

    #[test]
    fn encodes_gids_as_identity_h_hex() {
        let b = built(&[('A', 1), ('Z', 0x012C)]);
        let out =
            text_appearance_content_embedded("AZ", 12.0, 100.0, 20.0, 0, "0 g", "BPF0", &b, FONT);
        let s = String::from_utf8_lossy(&out);
        // Two-byte big-endian GID per char, shown as a hex string operator.
        assert!(s.contains("<0001012c>"), "expected GID hex, got: {s}");
        assert!(s.contains("Tj"), "expected a show operator, got: {s}");
        assert!(
            s.contains("/BPF0 12.00 Tf"),
            "expected the font op, got: {s}"
        );
    }

    #[test]
    fn skips_chars_without_a_glyph() {
        let b = built(&[('A', 1)]);
        // 'x' has no entry in gid_for -> skipped, matching drawText behavior.
        let out =
            text_appearance_content_embedded("AxA", 12.0, 100.0, 20.0, 0, "0 g", "BPF0", &b, FONT);
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains("<00010001>"),
            "expected the x to be skipped, got: {s}"
        );
    }
}
