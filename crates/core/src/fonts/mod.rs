//! Embed TrueType/OpenType fonts as PDF Type0/CIDFontType2 composite fonts.
pub mod cmap;
use ttf_parser::Face;

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
}
