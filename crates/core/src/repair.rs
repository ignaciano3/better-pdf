//! Recovery loader for PDFs that lopdf's strict parser rejects (broken or
//! missing xref/trailer, junk before the %PDF header, missing endobj/endstream
//! EOLs). Mirrors pdf-lib's approach: scan the raw bytes for indirect objects,
//! re-emit them verbatim with a freshly computed xref, and parse the rebuilt
//! file. Only invoked after a normal `Document::load_mem` has already failed.

use lopdf::Document;

/// One `N G obj … endobj` span found in the raw bytes.
struct RawObj {
    num: u32,
    generation: u16,
    /// Byte range of the whole span, `N G obj` through `endobj` inclusive
    /// (or through the byte before the next object header when `endobj` is
    /// missing).
    span: std::ops::Range<usize>,
}

pub(crate) fn repair_load(data: &[u8]) -> Result<Document, String> {
    let objs = scan_objects(data);
    if objs.is_empty() {
        return Err("repair failed: no indirect objects found".to_string());
    }
    let rebuilt = rebuild(data, &objs)?;
    Document::load_mem(&rebuilt).map_err(|e| format!("repair failed: {e}"))
}

/// Find every `N G obj` header outside stream data and delimit its span.
fn scan_objects(data: &[u8]) -> Vec<RawObj> {
    // Precompute every occurrence of the keywords `skip_stream` needs, once,
    // in a single linear pass each. Looking these up per-object with a fresh
    // "search to end of file" scan is O(objects * remaining bytes), which is
    // quadratic on malformed files with many small objects; binary-searching
    // these sorted position lists keeps each object's lookup O(log n).
    let stream_positions = find_all_keyword(data, b"stream");
    let endobj_positions = find_all_keyword(data, b"endobj");
    let endstream_positions = find_all_keyword(data, b"endstream");

    let mut headers: Vec<(usize, u32, u16, usize)> = Vec::new(); // (start, num, generation, body_start)
    let mut i = 0;
    while i < data.len() {
        if let Some((num, generation, header_start, body_start)) = parse_obj_header(data, i) {
            headers.push((header_start, num, generation, body_start));
            // Skip past stream data so `endstream`/`obj` bytes inside streams
            // are never mis-detected: jump to the `endstream` keyword if the
            // body opens a stream.
            i = skip_stream(
                body_start,
                &stream_positions,
                &endobj_positions,
                &endstream_positions,
            );
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (idx, &(start, num, generation, body_start)) in headers.iter().enumerate() {
        let hard_end = headers.get(idx + 1).map(|h| h.0).unwrap_or(data.len());
        // Span ends at `endobj` if present before the next header, else there.
        let end = find_keyword(&data[body_start..hard_end], b"endobj")
            .map(|p| body_start + p + b"endobj".len())
            .unwrap_or(hard_end);
        out.push(RawObj { num, generation, span: start..end });
    }
    out
}

/// Try to parse `N G obj` beginning at a digit at `pos` preceded by a
/// delimiter/whitespace (or start of file). Returns (num, gen, header_start,
/// body_start) — where body_start is the byte after the `obj` keyword.
fn parse_obj_header(data: &[u8], pos: usize) -> Option<(u32, u16, usize, usize)> {
    if !data[pos].is_ascii_digit() {
        return None;
    }
    if pos > 0 && !is_delim_or_ws(data[pos - 1]) {
        return None;
    }
    let mut i = pos;
    let num = read_uint(data, &mut i)?;
    skip_ws(data, &mut i)?;
    let generation = read_uint(data, &mut i)?;
    skip_ws(data, &mut i)?;
    if !data[i..].starts_with(b"obj") {
        return None;
    }
    Some((num as u32, generation as u16, pos, i + 3))
}

fn is_delim_or_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0' | b'>' | b']' | b')' | b'%')
}

fn read_uint(data: &[u8], i: &mut usize) -> Option<u64> {
    let start = *i;
    while *i < data.len() && data[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == start || *i - start > 10 {
        return None;
    }
    std::str::from_utf8(&data[start..*i]).ok()?.parse().ok()
}

/// At least one whitespace byte (incl. CR/LF), then skip the rest.
fn skip_ws(data: &[u8], i: &mut usize) -> Option<()> {
    let start = *i;
    while *i < data.len() && matches!(data[*i], b' ' | b'\t' | b'\r' | b'\n' | b'\0' | b'\x0c') {
        *i += 1;
    }
    (*i > start && *i < data.len()).then_some(())
}

/// If the object body contains a `stream` keyword before `endobj`, return the
/// index just past its matching `endstream`; else return `body_start`.
/// Searching for the literal `endstream` keyword (rather than trusting
/// /Length) is what makes missing-EOL and wrong-/Length files recoverable.
///
/// `stream_positions`/`endobj_positions`/`endstream_positions` are the sorted,
/// whole-file occurrence lists of each keyword (see `scan_objects`); this
/// looks up the first occurrence at or after `body_start` via binary search
/// instead of re-scanning from `body_start` to the end of the file.
fn skip_stream(
    body_start: usize,
    stream_positions: &[usize],
    endobj_positions: &[usize],
    endstream_positions: &[usize],
) -> usize {
    let Some(&stream_at) = first_at_or_after(stream_positions, body_start) else {
        return body_start;
    };
    // `endobj` before `stream` means the stream belongs to a later object.
    if let Some(&endobj_at) = first_at_or_after(endobj_positions, body_start)
        && endobj_at < stream_at
    {
        return body_start;
    }
    let stream_data_start = stream_at + b"stream".len();
    match first_at_or_after(endstream_positions, stream_data_start) {
        Some(&pos) => pos + b"endstream".len(),
        None => body_start,
    }
}

/// First element of a sorted position list that is `>= from`.
fn first_at_or_after(positions: &[usize], from: usize) -> Option<&usize> {
    let idx = positions.partition_point(|&p| p < from);
    positions.get(idx)
}

/// memmem: first occurrence of `needle` in `haystack`.
fn find_keyword(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// memmem: every (non-overlapping-aware, i.e. raw substring) occurrence of
/// `needle` in `haystack`, in ascending order. A single linear pass, used to
/// precompute keyword positions once instead of re-scanning per object.
fn find_all_keyword(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(i, w)| (w == needle).then_some(i))
        .collect()
}

/// Emit header + object spans (verbatim) + xref + trailer, recomputing all
/// offsets. Root is the /Type /Catalog object whose span starts latest in
/// the file (byte position order = chronological order for incremental
/// updates); /Info is recovered from the original trailer text when present.
fn rebuild(data: &[u8], objs: &[RawObj]) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;
    // Deduplicate by object number, last occurrence wins (incremental updates).
    let mut by_num: BTreeMap<u32, &RawObj> = BTreeMap::new();
    for o in objs {
        by_num.insert(o.num, o);
    }

    let mut by_span_start: Vec<&&RawObj> = by_num.values().collect();
    by_span_start.sort_by_key(|o| std::cmp::Reverse(o.span.start));
    let root = by_span_start
        .into_iter()
        .find(|o| contains_outside_stream(&data[o.span.clone()], b"/Catalog"))
        .map(|o| (o.num, o.generation))
        .ok_or("repair failed: no /Type /Catalog object found")?;
    let info = find_info_ref(data, objs, &by_num);

    let mut out: Vec<u8> = b"%PDF-1.7\n%\xC7\xEC\x8F\xA2\n".to_vec();
    let mut offsets: BTreeMap<u32, (u64, u16)> = BTreeMap::new();
    for (&num, o) in &by_num {
        offsets.insert(num, (out.len() as u64, o.generation));
        out.extend_from_slice(&data[o.span.clone()]);
        // Guarantee the span is properly terminated.
        if !out.ends_with(b"endobj") {
            out.extend_from_slice(b"\nendobj");
        }
        out.push(b'\n');
    }

    let xref_pos = out.len();
    let max_num = *offsets.keys().next_back().unwrap();
    out.extend_from_slice(format!("xref\n0 {}\n", max_num + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for num in 1..=max_num {
        match offsets.get(&num) {
            Some(&(off, generation)) => {
                out.extend_from_slice(format!("{off:010} {generation:05} n \n").as_bytes())
            }
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root {} {} R", max_num + 1, root.0, root.1).as_bytes(),
    );
    if let Some((n, g)) = info {
        out.extend_from_slice(format!(" /Info {n} {g} R").as_bytes());
    }
    out.extend_from_slice(format!(" >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes());
    Ok(out)
}

/// True when `needle` occurs in `span` before any `stream` keyword (so we
/// don't match bytes inside stream data).
fn contains_outside_stream(span: &[u8], needle: &[u8]) -> bool {
    let limit = find_keyword(span, b"stream").unwrap_or(span.len());
    find_keyword(&span[..limit], needle).is_some()
}

/// Recover `/Info N G R` from the original file's trailer text, keeping it
/// only when object N was actually found. Matches that fall inside any
/// detected object's span (which may include its stream payload) are
/// ignored, mirroring `contains_outside_stream`'s treatment of the catalog
/// search: an `/Info` byte sequence occurring inside stream data isn't a
/// real trailer reference.
fn find_info_ref(
    data: &[u8],
    objs: &[RawObj],
    by_num: &std::collections::BTreeMap<u32, &RawObj>,
) -> Option<(u32, u16)> {
    let in_any_span = |pos: usize| objs.iter().any(|o| o.span.contains(&pos));

    let mut search_from = 0;
    let mut last: Option<(u32, u16)> = None;
    while let Some(rel) = find_keyword(&data[search_from..], b"/Info") {
        let match_pos = search_from + rel;
        if in_any_span(match_pos) {
            search_from += rel + 1;
            continue;
        }
        let mut i = match_pos + b"/Info".len();
        let _ = skip_ws(data, &mut i);
        if let Some(num) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && let Some(generation) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && data.get(i) == Some(&b'R')
            && by_num.contains_key(&(num as u32))
        {
            last = Some((num as u32, generation as u16));
        }
        search_from += rel + 1;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    const JUST_METADATA: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/just_metadata.pdf");
    const MISSING_XREF: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/missing_xref_trailer_dict.pdf");
    const INVALID_OBJECTS: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/with_invalid_objects.pdf");
    const OFFSET_START: &[u8] =
        include_bytes!("../../../tests/fixtures/pdf-lib/PDF 2.0 with offset start.pdf");
    const BAD_ENDSTREAM: &[u8] = include_bytes!(
        "../../../tests/fixtures/pdf-lib/with_missing_endstream_eol_and_polluted_ctm.pdf"
    );

    fn page_count(doc: &lopdf::Document) -> usize {
        doc.get_pages().len()
    }

    #[test]
    fn repairs_missing_xref_trailer_dict() {
        let doc = repair_load(MISSING_XREF).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_just_metadata_and_preserves_info() {
        let doc = repair_load(JUST_METADATA).unwrap();
        assert_eq!(page_count(&doc), 1);
        // /Info must survive so getMetadata works (title is a hex string).
        let info = doc.trailer.get(b"Info").unwrap();
        assert!(matches!(info, lopdf::Object::Reference(_)));
    }

    #[test]
    fn repairs_offset_start() {
        let doc = repair_load(OFFSET_START).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_invalid_objects() {
        let doc = repair_load(INVALID_OBJECTS).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn repairs_missing_endstream_eol() {
        let doc = repair_load(BAD_ENDSTREAM).unwrap();
        assert!(page_count(&doc) >= 1);
    }

    #[test]
    fn garbage_still_fails() {
        assert!(repair_load(b"this is not a pdf at all").is_err());
    }
}
