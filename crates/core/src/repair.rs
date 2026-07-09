//! Recovery loader for PDFs that lopdf's strict parser rejects (broken or
//! missing xref/trailer, junk before the %PDF header, missing endobj/endstream
//! EOLs). Mirrors pdf-lib's approach: scan the raw bytes for indirect objects,
//! re-emit them verbatim with a freshly computed xref, and parse the rebuilt
//! file. Only invoked after a normal `Document::load_mem` has already failed.

use lopdf::{Document, Object, ObjectId};

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
    // Detect encryption in the ORIGINAL bytes before rebuilding: `rebuild`
    // only emits /Root and /Info in the fresh trailer, silently dropping any
    // /Encrypt entry from a file whose xref was merely broken (not actually
    // decrypted) — the strings/streams stay ciphertext while doc_io's
    // encryption gate is left with nothing to see. Mirror `find_info_ref`'s
    // "outside any object span" check: a trailer-only match, not one that
    // just happens to appear inside stream/string data.
    if find_encrypt_ref(data, &span_index(&objs)) {
        return Err(format!(
            "{} this PDF is encrypted; load it with PdfDocument.load(bytes, {{ password }}) (use \"\" for owner-locked files)",
            crate::doc_io::ENCRYPTED_PREFIX
        ));
    }
    let rebuilt = rebuild(data, &objs)?;
    let mut doc = Document::load_mem(&rebuilt).map_err(|e| format!("repair failed: {e}"))?;
    repair_page_tree(&mut doc);
    Ok(doc)
}

/// Re-point a broken catalog `/Pages` at the real page-tree root.
///
/// Some corrupt files parse but their catalog's `/Pages` reference names the
/// wrong object — e.g. pypdf iss2516, whose `/Pages` points at the Info dict
/// while the true `/Type /Pages` node sits at a different object number. When no
/// page resolves, locate the actual page-tree root among the recovered objects
/// (a `/Type /Pages` node that is not itself a kid of another) and wire the
/// catalog and the root's kids to it so page enumeration works again.
fn repair_page_tree(doc: &mut Document) {
    if !doc.get_pages().is_empty() {
        return;
    }
    // Every recovered /Type /Pages node.
    let pages_nodes: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| dict_type_is(o, b"Pages"))
        .map(|(id, _)| *id)
        .collect();
    if pages_nodes.is_empty() {
        return;
    }
    // Nodes referenced as another page node's kid can't be the tree root.
    let mut kid_ids: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    for &pid in &pages_nodes {
        if let Ok(d) = doc.get_dictionary(pid)
            && let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array())
        {
            for k in kids {
                if let Ok(r) = k.as_reference() {
                    kid_ids.insert(r);
                }
            }
        }
    }
    let Some(root_pages) = pages_nodes
        .iter()
        .copied()
        .find(|id| !kid_ids.contains(id))
        .or_else(|| pages_nodes.first().copied())
    else {
        return;
    };
    let Some(cat_id) = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
    else {
        return;
    };
    if let Ok(cat) = doc.get_object_mut(cat_id).and_then(Object::as_dict_mut) {
        cat.set("Pages", Object::Reference(root_pages));
    }
    // Point the root's kids' /Parent back at it, so downstream /Parent walks
    // (e.g. inherited attributes) stay consistent with the repaired /Pages.
    let kids: Vec<ObjectId> = doc
        .get_dictionary(root_pages)
        .ok()
        .and_then(|d| d.get(b"Kids").and_then(|o| o.as_array()).ok())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    for kid in kids {
        if let Ok(d) = doc.get_object_mut(kid).and_then(Object::as_dict_mut) {
            d.set("Parent", Object::Reference(root_pages));
        }
    }
}

/// True when `o` is a dictionary (or stream) whose `/Type` is the given name.
fn dict_type_is(o: &Object, ty: &[u8]) -> bool {
    o.as_dict()
        .ok()
        .and_then(|d| d.get(b"Type").ok())
        .and_then(|t| t.as_name().ok())
        == Some(ty)
}

/// Normalize a V4 (`/V 4`) `/Encrypt` dictionary that omits the top-level
/// `/Length` by injecting `/Length 128`, then rebuild the file with a fresh
/// xref. Returns `None` when the pattern doesn't apply (no classic trailer, no
/// V4 `/Encrypt`, or a top-level `/Length` already present) so the caller can
/// fall back to its original error.
///
/// Why this exists: PDF §7.6.1 fixes the V4 file-encryption-key length at 128
/// bits, so a conforming V4 `/Encrypt` need not carry `/Length`. lopdf 0.41,
/// however, derives the key length from the top-level `/Length` and defaults to
/// **40** bits when it is absent (`compute_file_encryption_key_r4`:
/// `self.length.unwrap_or(40)`), computing the wrong key and rejecting the
/// password. Making the field explicit (`/Length 128`) restores the correct
/// key. Invoked only after a decrypt attempt has already failed, so it can
/// never perturb the normal path — a bad rebuild just fails the retry too.
pub(crate) fn inject_v4_length(data: &[u8]) -> Option<Vec<u8>> {
    let (trailer_dict_start, trailer_dict_end) = find_trailer_dict(data)?;
    let trailer = &data[trailer_dict_start..trailer_dict_end];
    let encrypt_num = read_ref_num(trailer, b"/Encrypt")?;

    let objs = scan_objects(data);
    // Last occurrence of the object number wins (incremental updates).
    let enc = objs.iter().rev().find(|o| o.num == encrypt_num)?;
    let span = &data[enc.span.clone()];

    // The top-level dict opens at the first `<<` after the `obj` keyword.
    let open = find_keyword(span, b"<<")?;
    let after_open = open + 2;
    // Bail unless this is a V4 handler that is genuinely missing a top-level
    // `/Length` (injecting a duplicate key would be worse than the bug).
    if top_level_int(&span[after_open..], b"/V") != Some(4) {
        return None;
    }
    if top_level_has_key(&span[after_open..], b"/Length") {
        return None;
    }

    // Splice `/Length 128 ` in right after the top-level `<<`.
    let mut patched: Vec<u8> = Vec::with_capacity(span.len() + 13);
    patched.extend_from_slice(&span[..after_open]);
    patched.extend_from_slice(b" /Length 128");
    patched.extend_from_slice(&span[after_open..]);

    Some(rebuild_preserving_trailer(data, &objs, encrypt_num, &patched, trailer))
}

/// The `<< … >>` byte range of the file's last `trailer` dictionary, or `None`
/// for xref-stream files (which have no `trailer` keyword — deliberately
/// unsupported here so we never mangle a compressed cross-reference).
fn find_trailer_dict(data: &[u8]) -> Option<(usize, usize)> {
    let kw = data
        .windows(b"trailer".len())
        .rposition(|w| w == b"trailer")?;
    let mut i = kw + b"trailer".len();
    while i < data.len() && !data[i..].starts_with(b"<<") {
        if !matches!(data[i], b' ' | b'\t' | b'\r' | b'\n' | b'\0' | b'\x0c') {
            return None;
        }
        i += 1;
    }
    let start = i;
    // Balance `<<`/`>>` to find the matching close.
    let mut depth = 0usize;
    while i < data.len() {
        if data[i..].starts_with(b"<<") {
            depth += 1;
            i += 2;
        } else if data[i..].starts_with(b">>") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some((start, i));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Read `key N G R` from a dictionary's bytes, returning the object number `N`.
fn read_ref_num(dict: &[u8], key: &[u8]) -> Option<u32> {
    let at = find_keyword(dict, key)?;
    let mut i = at + key.len();
    skip_ws(dict, &mut i)?;
    let num = read_uint(dict, &mut i)?;
    skip_ws(dict, &mut i)?;
    let _generation = read_uint(dict, &mut i)?;
    skip_ws(dict, &mut i)?;
    (dict.get(i) == Some(&b'R')).then_some(num as u32)
}

/// True when `key` appears at brace-depth 1 within `body` (the bytes just after
/// a dict's opening `<<`), i.e. a direct entry rather than a nested one.
fn top_level_has_key(body: &[u8], key: &[u8]) -> bool {
    depth1_key_pos(body, key).is_some()
}

/// Read the integer value of a depth-1 `key` (e.g. `/V 4`), if present.
fn top_level_int(body: &[u8], key: &[u8]) -> Option<i64> {
    let at = depth1_key_pos(body, key)?;
    let mut i = at + key.len();
    skip_ws(body, &mut i)?;
    // Handle an optional leading sign, then digits.
    let neg = body.get(i) == Some(&b'-');
    if neg {
        i += 1;
    }
    let v = read_uint(body, &mut i)? as i64;
    Some(if neg { -v } else { v })
}

/// Position of `key` at brace-depth 1 within `body` (bytes after the opening
/// `<<` of the dict), skipping nested dictionaries.
fn depth1_key_pos(body: &[u8], key: &[u8]) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = 0;
    while i < body.len() {
        if body[i..].starts_with(b"<<") {
            depth += 1;
            i += 2;
        } else if body[i..].starts_with(b">>") {
            if depth == 1 {
                return None; // reached the end of this dict
            }
            depth -= 1;
            i += 2;
        } else if depth == 1 && body[i..].starts_with(key) {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

/// Re-emit every object (with `encrypt_num` replaced by `patched` bytes) under a
/// fresh xref, reusing the original `trailer` dict verbatim so `/ID` (which the
/// key derivation hashes) and `/Root`/`/Encrypt` survive byte-exact.
fn rebuild_preserving_trailer(
    data: &[u8],
    objs: &[RawObj],
    encrypt_num: u32,
    patched: &[u8],
    trailer: &[u8],
) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut by_num: BTreeMap<u32, &RawObj> = BTreeMap::new();
    for o in objs {
        by_num.insert(o.num, o); // last occurrence wins
    }

    let mut out: Vec<u8> = b"%PDF-1.7\n%\xC7\xEC\x8F\xA2\n".to_vec();
    let mut offsets: BTreeMap<u32, (u64, u16)> = BTreeMap::new();
    for (&num, o) in &by_num {
        offsets.insert(num, (out.len() as u64, o.generation));
        let bytes: &[u8] = if num == encrypt_num {
            patched
        } else {
            &data[o.span.clone()]
        };
        out.extend_from_slice(bytes);
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
    out.extend_from_slice(b"trailer\n");
    out.extend_from_slice(trailer);
    out.extend_from_slice(format!("\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes());
    out
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
    let info = find_info_ref(data, &span_index(objs), &by_num);

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

/// Sorted (span.start, span.end) pairs, precomputed once so membership tests
/// (`in_any_span`) are a `partition_point` binary search instead of a linear
/// scan over every object per match — matters on malformed files with many
/// small objects, where `/Info`/`/Encrypt` scanning would otherwise be
/// O(matches * objects).
fn span_index(objs: &[RawObj]) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = objs.iter().map(|o| (o.span.start, o.span.end)).collect();
    spans.sort_unstable_by_key(|&(start, _)| start);
    spans
}

/// True when `pos` falls inside some span in a `span_index()` result. Spans
/// don't overlap, so the last span starting at or before `pos` is the only
/// candidate.
fn in_any_span(spans: &[(usize, usize)], pos: usize) -> bool {
    let idx = spans.partition_point(|&(start, _)| start <= pos);
    idx > 0 && spans[idx - 1].1 > pos
}

/// True when an `/Encrypt` token appears in the original bytes outside every
/// detected object span (i.e. in trailer text, mirroring `find_info_ref`) and
/// is followed by either an indirect reference (`N G R`) or an inline
/// dictionary (`<<`). A broken-xref file that's genuinely encrypted still has
/// this token in its trailer even though its objects can't be parsed
/// normally; `rebuild` only emits /Root and /Info, so without this check an
/// encrypted-but-xref-broken file would come out of `repair_load` looking
/// like a plain, loadable (but still-ciphertext) document.
fn find_encrypt_ref(data: &[u8], spans: &[(usize, usize)]) -> bool {
    let mut search_from = 0;
    while let Some(rel) = find_keyword(&data[search_from..], b"/Encrypt") {
        let match_pos = search_from + rel;
        if in_any_span(spans, match_pos) {
            search_from += rel + 1;
            continue;
        }
        let mut i = match_pos + b"/Encrypt".len();
        let _ = skip_ws(data, &mut i);
        // Inline dictionary: `/Encrypt << ... >>`.
        if data[i..].starts_with(b"<<") {
            return true;
        }
        // Indirect reference: `/Encrypt N G R`.
        if let Some(_num) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && let Some(_generation) = read_uint(data, &mut i)
            && skip_ws(data, &mut i).is_some()
            && data.get(i) == Some(&b'R')
        {
            return true;
        }
        search_from += rel + 1;
    }
    false
}

/// Recover `/Info N G R` from the original file's trailer text, keeping it
/// only when object N was actually found. Matches that fall inside any
/// detected object's span (which may include its stream payload) are
/// ignored, mirroring `contains_outside_stream`'s treatment of the catalog
/// search: an `/Info` byte sequence occurring inside stream data isn't a
/// real trailer reference.
fn find_info_ref(
    data: &[u8],
    spans: &[(usize, usize)],
    by_num: &std::collections::BTreeMap<u32, &RawObj>,
) -> Option<(u32, u16)> {
    let mut search_from = 0;
    let mut last: Option<(u32, u16)> = None;
    while let Some(rel) = find_keyword(&data[search_from..], b"/Info") {
        let match_pos = search_from + rel;
        if in_any_span(spans, match_pos) {
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

    const CORRUPT_PAGES_REF: &[u8] =
        include_bytes!("../../../tests/fixtures/pypdf/issues/iss2516.pdf");

    #[test]
    fn repairs_catalog_pointing_pages_at_wrong_object() {
        // iss2516: the catalog's /Pages names the Info dict; the real /Type
        // /Pages node is a different object. Recovery must re-point /Pages so
        // the page tree resolves.
        let doc = repair_load(CORRUPT_PAGES_REF).unwrap();
        assert_eq!(page_count(&doc), 1);
    }

    #[test]
    fn load_pdf_recovers_corrupt_pages_reference() {
        // End-to-end: load_pdf sees a strict-parse "success" with zero pages and
        // must route through recovery (not return the empty doc).
        let doc = crate::doc_io::load_pdf(CORRUPT_PAGES_REF).unwrap();
        assert_eq!(page_count(&doc), 1);
    }

    const ENCRYPTED_MIN: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/encrypted-min.pdf");

    /// A broken-xref encrypted PDF must be rejected as encrypted, never
    /// "repaired" into a document that looks plaintext-loadable while its
    /// strings/streams are still ciphertext (the bug this module's
    /// `find_encrypt_ref` check exists to close).
    #[test]
    fn rejects_encrypted_pdf_with_broken_xref() {
        // Corrupt startxref so `Document::load_mem` fails and falls through to
        // the recovery loader, same as `doc_io::load_pdf` does on strict-parse
        // failure.
        let mut corrupted = ENCRYPTED_MIN.to_vec();
        let pos = find_keyword(&corrupted, b"startxref").expect("fixture has startxref");
        corrupted[pos..pos + b"startxref".len()].copy_from_slice(b"xxxxxxxxx");
        assert!(
            lopdf::Document::load_mem(&corrupted).is_err(),
            "corruption must actually break the strict parser"
        );

        let err = crate::doc_io::load_pdf(&corrupted)
            .expect_err("encrypted PDF with broken xref must not be silently repaired");
        assert!(
            err.starts_with(crate::doc_io::ENCRYPTED_PREFIX),
            "got: {err}"
        );
    }
}
