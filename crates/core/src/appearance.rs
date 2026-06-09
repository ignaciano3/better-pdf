//! Appearance engine: Helvetica metrics, WinAnsi encoding, and Form-XObject
//! construction for filled text/choice fields.

use lopdf::{Dictionary, Object, Stream};

/// Helvetica AFM advance widths (units / 1000 em) for WinAnsi codes 32..=126.
/// Index 0 == code 32 (space). Accented Latin-1 letters approximate to their
/// ASCII base width (good enough for v1 auto-sizing; corpus is Helvetica).
const HELV_ASCII: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32..47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48..63
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64..79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 80..95
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 96..111
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 112..126
];

/// Map a Latin-1/WinAnsi byte >=127 to an ASCII base letter for width purposes.
fn winansi_base(code: u8) -> u8 {
    match code {
        0xC0..=0xC5 => b'A',
        0xC8..=0xCB => b'E',
        0xCC..=0xCF => b'I',
        0xD2..=0xD6 => b'O',
        0xD9..=0xDC => b'U',
        0xD1 => b'N',
        0xC7 => b'C',
        0xE0..=0xE5 => b'a',
        0xE8..=0xEB => b'e',
        0xEC..=0xEF => b'i',
        0xF2..=0xF6 => b'o',
        0xF9..=0xFC => b'u',
        0xF1 => b'n',
        0xE7 => b'c',
        0xBF => b'?',
        0xA1 => b'!',
        _ => 0, // unknown
    }
}

/// Advance width (units/1000 em) of one WinAnsi byte.
pub fn helvetica_width(code: u8) -> u16 {
    if (32..=126).contains(&code) {
        HELV_ASCII[(code - 32) as usize]
    } else {
        let base = winansi_base(code);
        if base != 0 {
            HELV_ASCII[(base - 32) as usize]
        } else {
            556 // default average advance
        }
    }
}

/// Width of a WinAnsi byte string at the given font size (points).
pub fn string_width(bytes: &[u8], size: f32) -> f32 {
    let units: u32 = bytes.iter().map(|&c| helvetica_width(c) as u32).sum();
    units as f32 / 1000.0 * size
}

/// Encode a Rust string to WinAnsi bytes. The Latin-1 range (<=0xFF) maps by
/// code point; anything else becomes '?' (out of scope for v1's corpus).
// TODO(milestone): full WinAnsi 0x80-0x9F map (typographic punctuation).
pub fn encode_winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            let cp = c as u32;
            if cp <= 0xFF {
                cp as u8
            } else {
                b'?'
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
pub fn auto_size(da_size: f32, text: &[u8], avail_w: f32, box_h: f32) -> f32 {
    if da_size > 0.0 {
        return da_size;
    }
    // One ~2pt vertical breathing margin, capped to a sane max (Acrobat-like).
    let mut size = (box_h - 2.0).clamp(MIN_AUTO, MAX_AUTO);
    let w = string_width(text, size);
    if w > avail_w && w > 0.0 {
        size = (size * avail_w / w).max(MIN_AUTO);
    }
    size
}

/// Build the content stream for a single-line text appearance. `q` is the
/// quadding: 0=left, 1=center, 2=right. Coordinates are in the field's space
/// (BBox origin 0,0).
pub fn text_appearance_content(
    text: &[u8],
    size: f32,
    box_w: f32,
    box_h: f32,
    q: i64,
    color: &str,
    font: &str,
) -> Vec<u8> {
    let tw = string_width(text, size);
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

pub struct JpegInfo {
    pub width: i64,
    pub height: i64,
    pub color_space: &'static str,
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
            let color_space = if components == 1 {
                "DeviceGray"
            } else {
                "DeviceRGB"
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

pub fn build_signature_appearance_xobject(
    image_ref: lopdf::ObjectId,
    image_w: f32,
    image_h: f32,
    box_w: f32,
    box_h: f32,
) -> Stream {
    let scale = (box_w / image_w).min(box_h / image_h);
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
        assert_eq!(helvetica_width(b' '), 278);
        assert_eq!(helvetica_width(b'A'), 667);
        assert_eq!(helvetica_width(b'i'), 222);
        assert_eq!(helvetica_width(b'W'), 944);
    }

    #[test]
    fn string_width_scales_with_size() {
        // "AA" at size 10 = 2 * 667/1000 * 10 = 13.34
        let w = string_width(b"AA", 10.0);
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
        assert!((auto_size(0.0, b"AB", 300.0, 14.0) - 12.0).abs() < 0.01);
        // Narrow box forces shrink below the height cap.
        let s = auto_size(0.0, b"WWWWWWWWWW", 30.0, 14.0);
        assert!((4.0..12.0).contains(&s), "got {s}");
        // Explicit DA size is honored as-is.
        assert_eq!(auto_size(9.0, b"x", 300.0, 50.0), 9.0);
    }

    #[test]
    fn content_has_text_operators() {
        let c = text_appearance_content(b"Hi", 10.0, 100.0, 14.0, 0, "0 g", "Helv");
        let s = String::from_utf8(c).unwrap();
        assert!(s.contains("/Tx BMC"));
        assert!(s.contains("/Helv 10.00 Tf"));
        assert!(s.contains("(Hi) Tj"));
        assert!(s.contains("ET Q EMC"));
    }

    #[test]
    fn content_escapes_text() {
        let c = text_appearance_content(b"a(b)", 10.0, 100.0, 14.0, 0, "0 g", "Helv");
        assert!(String::from_utf8(c).unwrap().contains("(a\\(b\\)) Tj"));
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
}
