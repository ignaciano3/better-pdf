//! Build a ToUnicode CMap stream mapping CID (== glyph id, Identity encoding)
//! to Unicode scalar values, so viewers can copy/search rendered text.

/// `gid_to_unicode`: (glyph id used as 2-byte CID, original char). Returns the
/// decompressed CMap program; the caller wraps it in a stream.
pub fn to_unicode_cmap(gid_to_unicode: &[(u16, char)]) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("/CIDInit /ProcSet findresource begin\n");
    s.push_str("12 dict begin\nbegincmap\n");
    s.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.push_str("/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
    s.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    // bfchar in batches of <=100 per the CMap spec.
    for chunk in gid_to_unicode.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, ch) in chunk {
            // Encode Unicode as UTF-16BE hex (handles BMP + supplementary).
            let mut buf = [0u16; 2];
            let utf16 = ch.encode_utf16(&mut buf);
            let hex: String = utf16.iter().map(|u| format!("{u:04X}")).collect();
            s.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cmap_contains_codespace_and_bfchar() {
        let bytes = to_unicode_cmap(&[(3u16, 'A'), (10u16, 'é')]);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("begincodespacerange"));
        assert!(s.contains("<0000> <FFFF>"));
        assert!(s.contains("beginbfchar"));
        // gid 3 -> U+0041 'A'
        assert!(s.contains("<0003> <0041>"), "cmap was:\n{s}");
        // gid 10 -> U+00E9 'é'
        assert!(s.contains("<000A> <00E9>"), "cmap was:\n{s}");
    }
}
