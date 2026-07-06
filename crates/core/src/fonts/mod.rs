//! Embed TrueType/OpenType fonts as PDF Type0/CIDFontType2 composite fonts.
pub mod cmap;
use std::collections::{BTreeSet, HashMap};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};
use ttf_parser::Face;

use crate::fonts::cmap::to_unicode_cmap;

/// Font program plus which characters are actually used in the document.
pub struct EmbeddedFontInput<'a> {
    pub data: &'a [u8],
    pub subset: bool,
    pub used_chars: BTreeSet<char>,
}

/// What the caller needs after embedding: char->glyph mapping (for writing
/// Identity-H text).
pub struct BuiltFont {
    pub gid_for: HashMap<char, u16>,
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

    // Map used chars -> ORIGINAL glyph ids in the source font.
    let mut orig_gid_for: HashMap<char, u16> = HashMap::new();
    for &ch in &input.used_chars {
        if let Some(g) = face.glyph_index(ch) {
            orig_gid_for.insert(ch, g.0);
        }
    }

    // Font program + char->gid map. When subsetting, the `subsetter` crate REMAPS
    // glyph ids to a contiguous range (0->0 .notdef, then 1,2,3...), so the gids in
    // the embedded program differ from the source font. We therefore (a) translate
    // `gid_for` to the NEW ids and (b) emit /W and ToUnicode against those new ids,
    // keeping Identity-H + CIDToGIDMap Identity consistent with the embedded program.
    // When not subsetting, the original gids are used unchanged.
    let (program, gid_for): (Vec<u8>, HashMap<char, u16>) = if input.subset {
        let mut remapper = subsetter::GlyphRemapper::new();
        // Deterministic remap order: sort original gids so output is reproducible.
        let mut orig_gids: BTreeSet<u16> = BTreeSet::new();
        orig_gids.insert(0); // .notdef (also implicitly kept by the remapper)
        orig_gids.extend(orig_gid_for.values().copied());
        for g in &orig_gids {
            remapper.remap(*g);
        }
        let subset = subsetter::subset(input.data, 0, &remapper)
            .map_err(|e| format!("subset failed: {e}"))?;
        let new_gid_for: HashMap<char, u16> = orig_gid_for
            .iter()
            .filter_map(|(&ch, &g)| remapper.get(g).map(|ng| (ch, ng)))
            .collect();
        (subset, new_gid_for)
    } else {
        (input.data.to_vec(), orig_gid_for.clone())
    };

    // /W width array: [ gid [w] gid2 [w2] ... ] in /1000 em, only for used gids.
    // Advances are looked up by ORIGINAL gid on the source face; the emitted key is
    // the (possibly remapped) gid used in the embedded program. Sorted by emitted
    // gid for deterministic output.
    let mut w_entries: Vec<(u16, u16)> = orig_gid_for
        .iter()
        .filter_map(|(&ch, &orig)| gid_for.get(&ch).map(|&emit| (emit, orig)))
        .collect();
    w_entries.sort_by_key(|(emit, _)| *emit);
    let mut w_array: Vec<Object> = Vec::new();
    for (emit, orig) in w_entries {
        let adv = face
            .glyph_hor_advance(ttf_parser::GlyphId(orig))
            .unwrap_or(0);
        w_array.push(Object::Integer(emit as i64));
        w_array.push(Object::Array(vec![Object::Real(
            (adv as f32 * scale).round(),
        )]));
    }

    // FontFile2 stream with /Length1 = uncompressed program length.
    let len1 = program.len() as i64;
    let mut ff_dict = Dictionary::new();
    ff_dict.set("Length1", Object::Integer(len1));
    let ff_stream = Stream::new(ff_dict, program).with_compression(true);
    let ff_id = doc_add(Object::Stream(ff_stream));

    // Derive a PostScript-safe base name.
    let base = font_postscript_name(&face).unwrap_or_else(|| "Embedded".to_string());
    let base_name = if input.subset {
        format!("AAAAAA+{base}")
    } else {
        base.clone()
    };

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

    Ok((type0_id, BuiltFont { gid_for }))
}

/// Whether a missing glyph should abort the operation or be silently dropped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MissingGlyphPolicy {
    Error,
    Skip,
}

/// Map chars to GIDs per line ('\n'-split). `context` is e.g. "drawText on page 0"
/// or "field 'name'". Error format (STABLE, TS matches the prefix):
///   missing glyphs in font for {context}: "㐀" (U+3400), "丂" (U+4E02)
///
/// Excluded from the missing-glyph check: `\n` (line split) and any other
/// `char::is_control` (e.g. `\r`, `\t`). A missing *space* glyph IS an error
/// under `Error` (a font without space can't render sentences).
pub fn gids_per_line(
    built: &BuiltFont,
    text: &str,
    policy: MissingGlyphPolicy,
    context: &str,
) -> Result<Vec<Vec<u16>>, String> {
    let mut missing: BTreeSet<char> = Default::default();
    let lines: Vec<Vec<u16>> = text
        .split('\n')
        .map(|line| {
            line.chars()
                .filter_map(|c| {
                    if c.is_control() {
                        return None;
                    }
                    match built.gid_for.get(&c) {
                        Some(g) => Some(*g),
                        None => {
                            missing.insert(c);
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    if !missing.is_empty() && matches!(policy, MissingGlyphPolicy::Error) {
        let shown: Vec<String> = missing
            .iter()
            .take(8)
            .map(|c| format!("\"{c}\" (U+{:04X})", *c as u32))
            .collect();
        let more = missing.len().saturating_sub(8);
        let tail = if more > 0 {
            format!(", … and {more} more")
        } else {
            String::new()
        };
        return Err(format!(
            "missing glyphs in font for {context}: {}{tail}",
            shown.join(", ")
        ));
    }
    Ok(lines)
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

/// Word-wrap `text` for embedded `font` at `size` so each line fits `avail_w`.
/// Parses the face once and measures candidate runs locally (no per-word WASM
/// crossing), mirroring `measure_embedded`'s glyph-advance metric.
pub fn wrap_embedded(font: &[u8], size: f32, avail_w: f32, text: &str) -> Result<String, String> {
    let face = Face::parse(font, 0).map_err(|e| format!("invalid font: {e}"))?;
    let upem = face.units_per_em() as f32;
    if upem == 0.0 {
        return Err("font has zero unitsPerEm".to_string());
    }
    let measure = |s: &str| -> f32 {
        let units: u32 = s
            .chars()
            .map(|ch| {
                face.glyph_index(ch)
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .unwrap_or(0) as u32
            })
            .sum();
        units as f32 * size / upem
    };
    Ok(crate::appearance::wrap_str(text, avail_w, measure))
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] =
        include_bytes!("../../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

    #[test]
    fn gids_per_line_errors_on_missing_glyph_with_codepoint() {
        let mut gid_for = std::collections::HashMap::new();
        gid_for.insert('A', 1u16);
        let built = BuiltFont { gid_for };
        let err = gids_per_line(&built, "A㐀", MissingGlyphPolicy::Error, "drawText on page 0")
            .unwrap_err();
        assert!(err.starts_with("missing glyphs"), "got: {err}");
        assert!(err.contains("U+3400"), "got: {err}");
        assert!(err.contains("drawText on page 0"), "got: {err}");
    }

    #[test]
    fn gids_per_line_skip_matches_old_behavior_and_ignores_control_chars() {
        let mut gid_for = std::collections::HashMap::new();
        gid_for.insert('A', 1u16);
        let built = BuiltFont { gid_for };
        let lines = gids_per_line(&built, "A㐀\nA", MissingGlyphPolicy::Skip, "x").unwrap();
        assert_eq!(lines, vec![vec![1u16], vec![1u16]]);
        // control chars never error
        assert!(gids_per_line(&built, "A\tA", MissingGlyphPolicy::Error, "x").is_ok());
    }

    #[test]
    fn measures_text_width_positive_and_scales_with_size() {
        let w12 = measure_embedded(FONT, 12.0, "Hello").unwrap();
        let w24 = measure_embedded(FONT, 24.0, "Hello").unwrap();
        assert!(w12 > 0.0);
        assert!(
            (w24 - 2.0 * w12).abs() < 0.01,
            "width must scale linearly with size"
        );
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
        let input = EmbeddedFontInput {
            data: FONT,
            subset: false,
            used_chars: used,
        };
        let (font_id, built) = build_embedded_font(&mut add, &input).unwrap();

        let type0 = doc.get_object(font_id).unwrap().as_dict().unwrap();
        assert_eq!(type0.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
        assert_eq!(
            type0.get(b"Encoding").unwrap().as_name().unwrap(),
            b"Identity-H"
        );
        assert!(type0.has(b"ToUnicode"));
        // descendant CIDFontType2 present
        let desc = type0.get(b"DescendantFonts").unwrap().as_array().unwrap();
        let cid_ref = desc[0].as_reference().unwrap();
        let cid = doc.get_object(cid_ref).unwrap().as_dict().unwrap();
        assert_eq!(
            cid.get(b"Subtype").unwrap().as_name().unwrap(),
            b"CIDFontType2"
        );
        assert_eq!(
            cid.get(b"CIDToGIDMap").unwrap().as_name().unwrap(),
            b"Identity"
        );
        // glyph map covers used chars
        assert!(built.gid_for.contains_key(&'H'));
        assert!(built.gid_for.contains_key(&'é'));
    }

    #[test]
    fn subsetting_shrinks_and_preserves_gids() {
        // Build with subset=true and assert the embedded FontFile2 is smaller than
        // the original font, and that the glyph for 'H' still resolves in the subset.
        use lopdf::{Document, Object};
        let mut doc = Document::with_version("1.7");
        let mut add = |o: Object| doc.add_object(o);
        let used: std::collections::BTreeSet<char> = "Hé".chars().collect();
        let input = EmbeddedFontInput {
            data: FONT,
            subset: true,
            used_chars: used,
        };
        let (font_id, built) = build_embedded_font(&mut add, &input).unwrap();

        // Walk Type0 -> DescendantFonts -> FontDescriptor -> FontFile2
        let type0 = doc.get_object(font_id).unwrap().as_dict().unwrap();
        let cid = doc
            .get_object(
                type0.get(b"DescendantFonts").unwrap().as_array().unwrap()[0]
                    .as_reference()
                    .unwrap(),
            )
            .unwrap()
            .as_dict()
            .unwrap();
        let fd = doc
            .get_object(cid.get(b"FontDescriptor").unwrap().as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let ff = doc
            .get_object(fd.get(b"FontFile2").unwrap().as_reference().unwrap())
            .unwrap()
            .as_stream()
            .unwrap();
        let subset_len: i64 = ff.dict.get(b"Length1").unwrap().as_i64().unwrap();
        assert!(
            (subset_len as usize) < FONT.len(),
            "subset ({subset_len}) should be < original ({})",
            FONT.len()
        );

        // The subset font must still parse and contain the gid we recorded for 'H'.
        let raw = ff
            .decompressed_content()
            .unwrap_or_else(|_| ff.content.clone());
        let face = ttf_parser::Face::parse(&raw, 0).unwrap();
        let h_gid = built.gid_for[&'H'];
        assert!(
            face.glyph_hor_advance(ttf_parser::GlyphId(h_gid)).is_some(),
            "gid {h_gid} must survive subsetting with the same id"
        );
    }
}
