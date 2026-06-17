//! Embed TrueType/OpenType fonts as PDF Type0/CIDFontType2 composite fonts.
pub mod cmap;
use std::collections::{BTreeSet, HashMap};

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use ttf_parser::Face;

use crate::fonts::cmap::to_unicode_cmap;

/// Font program plus which characters are actually used in the document.
pub struct EmbeddedFontInput<'a> {
    pub data: &'a [u8],
    pub subset: bool,
    pub used_chars: BTreeSet<char>,
}

/// What the caller needs after embedding: char->glyph mapping (for writing
/// Identity-H text) and the font's design units per em (for measuring).
pub struct BuiltFont {
    pub gid_for: HashMap<char, u16>,
    pub units_per_em: u16,
}

/// Build the full Type0 object graph and return (Type0 dict id, BuiltFont).
/// `doc_add` adds an object to whichever document we're writing into, so this
/// works for both `Document` and `IncrementalDocument.new_document`.
///
/// This embeds the **full** font program; Task 5 swaps in a subset.
pub fn build_embedded_font(
    doc_add: &mut dyn FnMut(Object) -> ObjectId,
    input: &EmbeddedFontInput,
) -> Result<(ObjectId, BuiltFont), String> {
    let face = Face::parse(input.data, 0).map_err(|e| format!("invalid font: {e}"))?;
    let upem = face.units_per_em();
    if upem == 0 {
        return Err("font has zero unitsPerEm".into());
    }
    let scale = 1000.0 / upem as f32; // PDF glyph space is /1000 em

    // Map used chars -> glyph ids; collect the gid set for subsetting (Task 5).
    let mut gid_for: HashMap<char, u16> = HashMap::new();
    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0); // .notdef
    for &ch in &input.used_chars {
        if let Some(g) = face.glyph_index(ch) {
            gid_for.insert(ch, g.0);
            gids.insert(g.0);
        }
    }

    // Font program (full for now; Task 5 swaps in the subset built from `gids`).
    let program: Vec<u8> = input.data.to_vec();

    // /W width array: [ gid [w] gid2 [w2] ... ] in /1000 em, only for used gids.
    let mut w_array: Vec<Object> = Vec::new();
    for (_, &g) in gid_for.iter() {
        let adv = face.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0);
        w_array.push(Object::Integer(g as i64));
        w_array.push(Object::Array(vec![Object::Real((adv as f32 * scale).round())]));
    }

    // FontFile2 stream with /Length1 = uncompressed program length.
    let len1 = program.len() as i64;
    let mut ff_dict = Dictionary::new();
    ff_dict.set("Length1", Object::Integer(len1));
    let ff_stream = Stream::new(ff_dict, program).with_compression(true);
    let ff_id = doc_add(Object::Stream(ff_stream));

    // Derive a PostScript-safe base name.
    let base = font_postscript_name(&face).unwrap_or_else(|| "Embedded".to_string());
    let base_name = if input.subset { format!("AAAAAA+{base}") } else { base.clone() };

    let bbox = face.global_bounding_box();
    let flags = 4i64; // Symbolic; refine if needed.
    let descriptor = dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(base_name.as_bytes().to_vec()),
        "Flags" => Object::Integer(flags),
        "FontBBox" => Object::Array(vec![
            Object::Real(bbox.x_min as f32 * scale),
            Object::Real(bbox.y_min as f32 * scale),
            Object::Real(bbox.x_max as f32 * scale),
            Object::Real(bbox.y_max as f32 * scale),
        ]),
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => Object::Real(face.ascender() as f32 * scale),
        "Descent" => Object::Real(face.descender() as f32 * scale),
        "CapHeight" => Object::Real(face.capital_height().unwrap_or(face.ascender()) as f32 * scale),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(ff_id),
    };
    let descriptor_id = doc_add(Object::Dictionary(descriptor));

    let cid_font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "CIDToGIDMap" => Object::Name(b"Identity".to_vec()),
        "DW" => Object::Integer(1000),
        "W" => Object::Array(w_array),
    };
    let cid_id = doc_add(Object::Dictionary(cid_font));

    // ToUnicode CMap (sorted by gid for deterministic output).
    let mut pairs: Vec<(u16, char)> = gid_for.iter().map(|(c, g)| (*g, *c)).collect();
    pairs.sort_by_key(|(g, _)| *g);
    let tu = to_unicode_cmap(&pairs);
    let tu_id = doc_add(Object::Stream(
        Stream::new(Dictionary::new(), tu).with_compression(true),
    ));

    let type0 = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(cid_id)]),
        "ToUnicode" => Object::Reference(tu_id),
    };
    let type0_id = doc_add(Object::Dictionary(type0));

    Ok((type0_id, BuiltFont { gid_for, units_per_em: upem }))
}

/// Read the font's PostScript name (name id 6), if present.
fn font_postscript_name(face: &Face) -> Option<String> {
    face.names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .and_then(|n| n.to_string())
}

/// Width in points of `text` rendered in `font` at `size`. Sums horizontal
/// advances of each char's glyph, scaled by size / unitsPerEm. Chars with no
/// glyph contribute the font's default advance (or 0 if none).
pub fn measure_embedded(font: &[u8], size: f32, text: &str) -> Result<f32, String> {
    let face = Face::parse(font, 0).map_err(|e| format!("invalid font: {e}"))?;
    let upem = face.units_per_em() as f32;
    if upem == 0.0 {
        return Err("font has zero unitsPerEm".to_string());
    }
    let mut units = 0u32;
    for ch in text.chars() {
        if let Some(gid) = face.glyph_index(ch) {
            units += face.glyph_hor_advance(gid).unwrap_or(0) as u32;
        }
    }
    Ok(units as f32 * size / upem)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

    #[test]
    fn measures_text_width_positive_and_scales_with_size() {
        let w12 = measure_embedded(FONT, 12.0, "Hello").unwrap();
        let w24 = measure_embedded(FONT, 24.0, "Hello").unwrap();
        assert!(w12 > 0.0);
        assert!((w24 - 2.0 * w12).abs() < 0.01, "width must scale linearly with size");
    }

    #[test]
    fn measure_rejects_garbage_font() {
        assert!(measure_embedded(b"not a font", 12.0, "x").is_err());
    }

    #[test]
    fn builds_type0_font_graph() {
        use lopdf::{Document, Object};
        let mut doc = Document::with_version("1.7");
        let mut add = |o: Object| doc.add_object(o);
        let used: std::collections::BTreeSet<char> = "Hé".chars().collect();
        let input = EmbeddedFontInput { data: FONT, subset: false, used_chars: used };
        let (font_id, built) = build_embedded_font(&mut add, &input).unwrap();

        let type0 = doc.get_object(font_id).unwrap().as_dict().unwrap();
        assert_eq!(type0.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
        assert_eq!(type0.get(b"Encoding").unwrap().as_name().unwrap(), b"Identity-H");
        assert!(type0.has(b"ToUnicode"));
        // descendant CIDFontType2 present
        let desc = type0.get(b"DescendantFonts").unwrap().as_array().unwrap();
        let cid_ref = desc[0].as_reference().unwrap();
        let cid = doc.get_object(cid_ref).unwrap().as_dict().unwrap();
        assert_eq!(cid.get(b"Subtype").unwrap().as_name().unwrap(), b"CIDFontType2");
        assert_eq!(cid.get(b"CIDToGIDMap").unwrap().as_name().unwrap(), b"Identity");
        // glyph map covers used chars
        assert!(built.gid_for.contains_key(&'H'));
        assert!(built.gid_for.contains_key(&'é'));
    }
}
