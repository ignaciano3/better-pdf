//! Embed a page from another PDF as a Form XObject (deep-copies the page's
//! content + resource subtree into the target document).
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;

/// Recursively copy `src_id` and everything it references from `src` into `dst`,
/// remapping object ids. Idempotent via `map`. Breaks cycles by inserting the
/// new id into `map` BEFORE recursing into the object's children.
pub fn import_object_tree(
    dst: &mut Document,
    src: &Document,
    src_id: ObjectId,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<ObjectId, String> {
    if let Some(&new_id) = map.get(&src_id) {
        return Ok(new_id);
    }
    let new_id = dst.new_object_id();
    // Insert BEFORE recursing so cyclic references resolve to this id.
    map.insert(src_id, new_id);
    let obj = src.get_object(src_id).map_err(|e| e.to_string())?.clone();
    let rewritten = import_object(dst, src, obj, map)?;
    dst.objects.insert(new_id, rewritten);
    Ok(new_id)
}

/// Deep-copy a single Object value, importing any nested references.
fn import_object(
    dst: &mut Document,
    src: &Document,
    obj: Object,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Object, String> {
    Ok(match obj {
        Object::Reference(id) => Object::Reference(import_object_tree(dst, src, id, map)?),
        Object::Array(a) => Object::Array(
            a.into_iter()
                .map(|o| import_object(dst, src, o, map))
                .collect::<Result<_, _>>()?,
        ),
        Object::Dictionary(d) => Object::Dictionary(import_dict(dst, src, d, map)?),
        Object::Stream(mut s) => {
            s.dict = import_dict(dst, src, s.dict, map)?;
            Object::Stream(s)
        }
        other => other,
    })
}

fn import_dict(
    dst: &mut Document,
    src: &Document,
    d: Dictionary,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Result<Dictionary, String> {
    let mut out = Dictionary::new();
    for (k, v) in d.into_iter() {
        out.set(k, import_object(dst, src, v, map)?);
    }
    Ok(out)
}

/// Load `src_bytes`, take page `src_page_index`, and build a Form XObject in
/// `dst` that draws that page. Returns (xobject_id, width, height) in PDF units.
pub fn embed_page_as_xobject(
    dst: &mut Document,
    src_bytes: &[u8],
    src_page_index: usize,
) -> Result<(ObjectId, f32, f32), String> {
    let src = Document::load_mem(src_bytes).map_err(|e| e.to_string())?;
    let page_ids: Vec<ObjectId> = src.get_pages().into_values().collect();
    let page_id = *page_ids
        .get(src_page_index)
        .ok_or_else(|| format!("source page {src_page_index} out of range"))?;

    // MediaBox (resolve inherited by walking /Parent if absent).
    let media = resolve_media_box(&src, page_id)?;
    let (x0, y0, x1, y1) = (media[0], media[1], media[2], media[3]);
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0.0 || h <= 0.0 {
        return Err("source page has invalid MediaBox".to_string());
    }

    // Concatenate decompressed content streams.
    let mut content: Vec<u8> = Vec::new();
    for cid in src.get_page_contents(page_id) {
        if let Ok(obj) = src.get_object(cid) {
            if let Ok(stream) = obj.as_stream() {
                let bytes = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                content.extend_from_slice(&bytes);
                content.push(b'\n');
            }
        }
    }

    // Import the page's resolved Resources subtree (deep copy into dst).
    let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
    let resources_obj: Object = {
        let page_dict = src.get_dictionary(page_id).map_err(|e| e.to_string())?;
        match page_dict.get(b"Resources") {
            Ok(Object::Reference(rid)) => {
                Object::Reference(import_object_tree(dst, &src, *rid, &mut map)?)
            }
            Ok(Object::Dictionary(d)) => {
                Object::Dictionary(import_dict(dst, &src, d.clone(), &mut map)?)
            }
            _ => match inherited_resources(&src, page_id) {
                Some(Object::Reference(rid)) => {
                    Object::Reference(import_object_tree(dst, &src, rid, &mut map)?)
                }
                Some(Object::Dictionary(d)) => {
                    Object::Dictionary(import_dict(dst, &src, d, &mut map)?)
                }
                _ => Object::Dictionary(Dictionary::new()),
            },
        }
    };

    let form = dictionary! {
        "Type" => Object::Name(b"XObject".to_vec()),
        "Subtype" => Object::Name(b"Form".to_vec()),
        "FormType" => Object::Integer(1),
        "BBox" => Object::Array(vec![
            Object::Real(0.0), Object::Real(0.0), Object::Real(w), Object::Real(h),
        ]),
        "Matrix" => Object::Array(vec![
            Object::Real(1.0), Object::Real(0.0), Object::Real(0.0),
            Object::Real(1.0), Object::Real(-x0), Object::Real(-y0),
        ]),
        "Resources" => resources_obj,
    };
    let mut stream = lopdf::Stream::new(form, content);
    // Best-effort compression; uncompressed is valid if it fails.
    stream.compress().ok();
    let xid = dst.add_object(Object::Stream(stream));
    Ok((xid, w, h))
}

fn resolve_media_box(src: &Document, page_id: ObjectId) -> Result<[f32; 4], String> {
    let mut cur = Some(page_id);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        let d = src.get_dictionary(id).map_err(|e| e.to_string())?;
        if let Ok(mb) = d.get(b"MediaBox") {
            let resolved = src.dereference(mb).map_err(|e| e.to_string())?.1;
            let arr = resolved.as_array().map_err(|e| e.to_string())?;
            if arr.len() == 4 {
                let f = |o: &Object| {
                    src.dereference(o)
                        .ok()
                        .and_then(|(_, v)| v.as_float().ok())
                        .unwrap_or(0.0)
                };
                return Ok([f(&arr[0]), f(&arr[1]), f(&arr[2]), f(&arr[3])]);
            }
        }
        cur = d.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Err("source page has no MediaBox".to_string())
}

fn inherited_resources(src: &Document, page_id: ObjectId) -> Option<Object> {
    let mut cur = src
        .get_dictionary(page_id)
        .ok()?
        .get(b"Parent")
        .and_then(Object::as_reference)
        .ok();
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        let d = src.get_dictionary(id).ok()?;
        if let Ok(r) = d.get(b"Resources") {
            return Some(r.clone());
        }
        cur = d.get(b"Parent").and_then(Object::as_reference).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const SRC: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn embeds_page_as_form_xobject() {
        let mut dst = Document::with_version("1.7");
        let (xid, w, h) = embed_page_as_xobject(&mut dst, SRC, 0).unwrap();
        assert!(w > 0.0 && h > 0.0);
        let xobj = dst.get_object(xid).unwrap().as_stream().unwrap();
        assert_eq!(
            xobj.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
        assert!(xobj.dict.has(b"BBox"));
        assert!(
            xobj.dict.has(b"Resources"),
            "form must carry copied resources"
        );
        let content = xobj
            .decompressed_content()
            .unwrap_or_else(|_| xobj.content.clone());
        assert!(!content.is_empty(), "form content must be non-empty");
    }

    #[test]
    fn embed_rejects_page_out_of_range() {
        let mut dst = Document::with_version("1.7");
        assert!(embed_page_as_xobject(&mut dst, SRC, 9999).is_err());
    }

    #[test]
    fn import_object_tree_dedupes_shared_refs() {
        let src = Document::load_mem(SRC).unwrap();
        let mut dst = Document::with_version("1.7");
        let mut map = std::collections::HashMap::new();
        let (_, page_id) = src.get_pages().into_iter().next().unwrap();
        let a = import_object_tree(&mut dst, &src, page_id, &mut map).unwrap();
        let b = import_object_tree(&mut dst, &src, page_id, &mut map).unwrap();
        assert_eq!(a, b, "second import of same id must return cached new id");
    }
}
