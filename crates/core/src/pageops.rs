//! Assemble a new PDF from an ordered selection of pages across source PDFs.
//!
//! A single primitive — [`manipulate_pages_json`] — builds a brand-new PDF from
//! an ordered list of `{doc, page}` selections drawn from one or more source
//! documents. Merge, extract, reorder, remove and split all reduce to this.
use lopdf::{dictionary, Document, Object, ObjectId};
use serde::Deserialize;

#[derive(Deserialize)]
struct DocDesc {
    offset: usize,
    length: usize,
}

#[derive(Deserialize)]
struct Sel {
    doc: usize,
    page: usize,
}

/// Attributes a /Page may inherit from an ancestor /Pages node.
const INHERITABLE: &[&[u8]] = &[b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Walk the page's /Parent chain; for each inheritable key the page lacks,
/// collect the nearest ancestor's value (references are resolved one level so
/// the carried value is self-contained-ish). A cycle guard bounds the walk.
fn resolve_inherited(doc: &Document, page_id: ObjectId) -> Vec<(Vec<u8>, Object)> {
    let mut found: Vec<(Vec<u8>, Object)> = Vec::new();
    let mut current = Some(page_id);
    let mut guard = 0;
    while let Some(id) = current {
        guard += 1;
        if guard > 64 {
            break; // cycle guard
        }
        let dict = match doc.get_dictionary(id) {
            Ok(d) => d,
            Err(_) => break,
        };
        for &key in INHERITABLE {
            if found.iter().any(|(k, _)| k == key) {
                continue;
            }
            if let Ok(v) = dict.get(key) {
                let resolved = match v {
                    Object::Reference(r) => doc.get_object(*r).cloned().unwrap_or_else(|_| v.clone()),
                    other => other.clone(),
                };
                found.push((key.to_vec(), resolved));
            }
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    found
}

/// Assemble a new PDF from an ordered page selection across the source PDFs
/// packed into `docs_blob`.
///
/// * `docs_blob` — the concatenated bytes of every source PDF.
/// * `docs_json` — JSON array of `{"offset","length"}` slicing `docs_blob` into docs.
/// * `plan_json` — JSON array of `{"doc","page"}` (both 0-based) giving the
///   ordered output pages. Duplicates are allowed and yield distinct pages.
pub fn manipulate_pages_json(
    docs_blob: &[u8],
    docs_json: &str,
    plan_json: &str,
) -> Result<Vec<u8>, String> {
    let descs: Vec<DocDesc> = serde_json::from_str(docs_json).map_err(|e| format!("invalid docs: {e}"))?;
    let plan: Vec<Sel> = serde_json::from_str(plan_json).map_err(|e| format!("invalid plan: {e}"))?;
    if plan.is_empty() {
        return Err("no pages selected".to_string());
    }

    let mut merged = Document::with_version("1.7");
    let mut next: u32 = 1;
    let mut per_doc_pages: Vec<Vec<ObjectId>> = Vec::new();

    for d in &descs {
        let end = d
            .offset
            .checked_add(d.length)
            .ok_or("doc range out of bounds")?;
        if end > docs_blob.len() {
            return Err("doc range out of bounds".to_string());
        }
        let mut doc = Document::load_mem(&docs_blob[d.offset..end]).map_err(|e| e.to_string())?;

        // Resolve inherited attrs onto each page BEFORE renumber/move, while the
        // /Parent chain is still intact. Only set keys the page itself lacks.
        let pre_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        for &pid in &pre_ids {
            let inh = resolve_inherited(&doc, pid);
            if let Ok(pd) = doc.get_dictionary_mut(pid) {
                for (k, v) in inh {
                    if !pd.has(&k) {
                        pd.set(k, v);
                    }
                }
            }
        }

        // Shift this doc's object ids (and every internal reference) into a
        // disjoint range starting at `next`, then advance `next`.
        doc.renumber_objects_with(next);
        next = doc.max_id + 1;

        // Record page ids AFTER renumber (these are now in merged-id space).
        let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        per_doc_pages.push(page_ids);

        // Bulk-move every object into the merged doc.
        merged.objects.extend(std::mem::take(&mut doc.objects));
    }

    // CRITICAL: set max_id from the loop's final `next` BEFORE any
    // `new_object_id`/`add_object` call, so fresh ids never collide with the
    // moved objects.
    merged.max_id = next.saturating_sub(1);

    let pages_id = merged.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(plan.len());
    let mut used: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    for s in &plan {
        let pages = per_doc_pages
            .get(s.doc)
            .ok_or_else(|| format!("doc index {} out of range", s.doc))?;
        let src_pid = *pages
            .get(s.page)
            .ok_or_else(|| format!("page index {} out of range", s.page))?;
        // A page selected more than once must become a distinct object so the
        // output tree has independent /Parent links. Shallow-clone the page
        // dict (shared Contents/Resources references are fine).
        let pid = if used.contains(&src_pid) {
            let cloned = merged.get_dictionary(src_pid).map_err(|e| e.to_string())?.clone();
            merged.add_object(Object::Dictionary(cloned))
        } else {
            used.insert(src_pid);
            src_pid
        };
        if let Ok(pd) = merged.get_dictionary_mut(pid) {
            pd.set("Parent", Object::Reference(pages_id));
        }
        kids.push(Object::Reference(pid));
    }

    let count = kids.len() as i64;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(count),
        }),
    );
    let catalog_id = merged.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    merged.trailer.set("Root", Object::Reference(catalog_id));

    // Drop the old per-source catalogs/pages-trees and any unselected pages.
    // Everything reachable from the new Root (Pages tree + selected pages +
    // their content/resources/annots) is retained.
    merged.prune_objects();

    let mut out = Vec::new();
    merged.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn page_count(bytes: &[u8]) -> usize {
        Document::load_mem(bytes).unwrap().get_pages().len()
    }

    // Concatenate sources, build the docs_json table.
    fn pack(docs: &[&[u8]]) -> (Vec<u8>, String) {
        let mut blob = Vec::new();
        let mut table = String::from("[");
        for (i, d) in docs.iter().enumerate() {
            if i > 0 {
                table.push(',');
            }
            table.push_str(&format!(r#"{{"offset":{},"length":{}}}"#, blob.len(), d.len()));
            blob.extend_from_slice(d);
        }
        table.push(']');
        (blob, table)
    }

    #[test]
    fn merge_two_copies_doubles_page_count() {
        let n = page_count(FICHA);
        let (blob, docs) = pack(&[FICHA, FICHA]);
        // plan = all pages of doc 0 then all pages of doc 1
        let mut plan = String::from("[");
        for d in 0..2 {
            for p in 0..n {
                if !(d == 0 && p == 0) {
                    plan.push(',');
                }
                plan.push_str(&format!(r#"{{"doc":{d},"page":{p}}}"#));
            }
        }
        plan.push(']');
        let out = manipulate_pages_json(&blob, &docs, &plan).unwrap();
        assert_eq!(page_count(&out), 2 * n);
    }

    #[test]
    fn extract_single_page() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":0}]"#).unwrap();
        assert_eq!(page_count(&out), 1);
        // MediaBox present on the extracted page (inherited attrs resolved)
        let doc = Document::load_mem(&out).unwrap();
        let (_, pid) = doc.get_pages().into_iter().next().unwrap();
        assert!(
            doc.get_dictionary(pid).unwrap().has(b"MediaBox"),
            "extracted page must carry MediaBox"
        );
    }

    #[test]
    fn reorder_preserves_count() {
        let n = page_count(FICHA);
        if n >= 2 {
            let (blob, docs) = pack(&[FICHA]);
            let out = manipulate_pages_json(
                &blob,
                &docs,
                r#"[{"doc":0,"page":1},{"doc":0,"page":0}]"#,
            )
            .unwrap();
            assert_eq!(page_count(&out), 2);
        }
    }

    #[test]
    fn errors_on_empty_plan() {
        let (blob, docs) = pack(&[FICHA]);
        assert!(manipulate_pages_json(&blob, &docs, "[]").is_err());
    }

    #[test]
    fn errors_on_page_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":0,"page":9999}]"#);
        assert!(r.unwrap_err().contains("page"));
    }

    #[test]
    fn errors_on_doc_out_of_range() {
        let (blob, docs) = pack(&[FICHA]);
        let r = manipulate_pages_json(&blob, &docs, r#"[{"doc":5,"page":0}]"#);
        assert!(r.unwrap_err().contains("doc"));
    }

    #[test]
    fn duplicate_page_selection_produces_two_distinct_pages() {
        let (blob, docs) = pack(&[FICHA]);
        let out = manipulate_pages_json(
            &blob,
            &docs,
            r#"[{"doc":0,"page":0},{"doc":0,"page":0}]"#,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ids: Vec<_> = doc.get_pages().into_values().collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(
            ids[0], ids[1],
            "duplicate selection must yield distinct page objects"
        );
    }
}
