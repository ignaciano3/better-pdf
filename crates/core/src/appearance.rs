//! Appearance engine: Helvetica metrics, WinAnsi encoding, and Form-XObject
//! construction for filled text/choice fields.

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use lopdf::{Dictionary, Object, Stream};
use std::io::{Read, Write};

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

const PAD: f32 = 2.0;
const MAX_AUTO: f32 = 12.0;
const MIN_AUTO: f32 = 4.0;

/// Parsed default-appearance string.
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
    let tx = match q {
        1 => ((box_w - tw) / 2.0).max(PAD), // center
        2 => (box_w - PAD - tw).max(PAD),   // right
        _ => PAD,                           // left
    };
    let ty = ((box_h - size) / 2.0 + size * 0.2).max(PAD);
    let escaped = escape_pdf_literal(text);
    let mut out = Vec::new();
    out.extend_from_slice(b"/Tx BMC q BT ");
    out.extend_from_slice(format!("/{font} {size:.2} Tf {color} ").as_bytes());
    out.extend_from_slice(format!("{tx:.2} {ty:.2} Td (").as_bytes());
    out.extend_from_slice(&escaped);
    out.extend_from_slice(b") Tj ET Q EMC");
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
    Stream::new(dict, content).with_compression(false)
}

#[derive(Debug)]
pub struct JpegInfo {
    pub width: i64,
    pub height: i64,
    pub color_space: &'static str,
}

#[derive(Clone, Debug)]
pub enum SignatureImage {
    Jpeg { data: Vec<u8>, info: ImageInfo },
    Raw { data: Vec<u8>, info: ImageInfo },
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

pub fn build_jpeg_image_xobject(data: Vec<u8>, info: &JpegInfo) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(info.width));
    dict.set("Height", Object::Integer(info.height));
    dict.set(
        "ColorSpace",
        Object::Name(info.color_space.as_bytes().to_vec()),
    );
    dict.set("BitsPerComponent", Object::Integer(8));
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    Stream::new(dict, data).with_compression(false)
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
        SignatureImage::Raw { data, info } => build_raw_image_xobject(data, &info),
    }
}

fn build_raw_image_xobject(data: Vec<u8>, info: &ImageInfo) -> Stream {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).expect("Vec writes cannot fail");
    let compressed = encoder.finish().expect("zlib finish cannot fail for Vec");

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(info.width));
    dict.set("Height", Object::Integer(info.height));
    dict.set(
        "ColorSpace",
        Object::Name(info.color_space.as_bytes().to_vec()),
    );
    dict.set("BitsPerComponent", Object::Integer(8));
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    Stream::new(dict, compressed).with_compression(false)
}

fn png_image(data: &[u8]) -> Result<SignatureImage, String> {
    let mut pos = 8usize;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut idat = Vec::new();

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
            b"IDAT" => idat.extend_from_slice(chunk),
            b"IEND" => break,
            _ => {}
        }
    }

    if width == 0 || height == 0 {
        return Err("invalid PNG dimensions".to_string());
    }
    if bit_depth != 8 {
        return Err("only 8-bit PNG signatures are supported".to_string());
    }
    if interlace != 0 {
        return Err("interlaced PNG signatures are not supported".to_string());
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
        std::mem::swap(&mut prev, &mut cur);
    }

    Ok(SignatureImage::Raw {
        data: out,
        info: ImageInfo {
            width: width as i64,
            height: height as i64,
            color_space,
        },
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
    Stream::new(dict, content).with_compression(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
