//! Document outline (bookmarks) tree: build a `/Outlines` hierarchy and wire it
//! into the catalog, both on loaded PDFs (incremental update) and on documents
//! created from scratch.

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId};
use serde::Deserialize;

/// One outline (bookmark) entry. `page` is a 0-based page index; `children`
/// nest sub-bookmarks.
#[derive(Deserialize)]
pub struct OutlineItem {
    pub title: String,
    pub page: usize,
    #[serde(default)]
    pub children: Vec<OutlineItem>,
}

/// Count of all descendant items (immediate children + their descendants).
fn total_descendants(items: &[OutlineItem]) -> i64 {
    let mut n = 0i64;
    for it in items {
        n += 1 + total_descendants(&it.children);
    }
    n
}

/// Recursively validate that every item's page index is `< page_count`.
pub fn validate_pages(items: &[OutlineItem], page_count: usize) -> Result<(), String> {
    for it in items {
        if it.page >= page_count {
            return Err(format!(
                "outline page {} out of range ({page_count} pages)",
                it.page
            ));
        }
        validate_pages(&it.children, page_count)?;
    }
    Ok(())
}

/// Build the sibling chain of `items` under `parent` and insert each item's
/// dictionary into `doc`. Returns `(first_id, last_id)` of the chain, or `None`
/// if `items` is empty. `page_ref` maps a page index to its `ObjectId`.
fn build_siblings(
    doc: &mut Document,
    items: &[OutlineItem],
    parent: ObjectId,
    page_ref: &dyn Fn(usize) -> Option<ObjectId>,
) -> Result<Option<(ObjectId, ObjectId)>, String> {
    if items.is_empty() {
        return Ok(None);
    }

    // Reserve an id for each sibling so we can link Next/Prev.
    let ids: Vec<ObjectId> = items.iter().map(|_| doc.new_object_id()).collect();

    for (i, item) in items.iter().enumerate() {
        let id = ids[i];

        // Recurse into children with this item as their parent.
        let child_range = build_siblings(doc, &item.children, id, page_ref)?;

        let page_id = page_ref(item.page)
            .ok_or_else(|| format!("outline page {} out of range", item.page))?;

        let mut dict = Dictionary::new();
        dict.set("Title", Object::string_literal(item.title.as_bytes().to_vec()));
        dict.set("Parent", Object::Reference(parent));
        dict.set(
            "Dest",
            Object::Array(vec![
                Object::Reference(page_id),
                Object::Name(b"XYZ".to_vec()),
                Object::Null,
                Object::Null,
                Object::Null,
            ]),
        );
        if i > 0 {
            dict.set("Prev", Object::Reference(ids[i - 1]));
        }
        if i + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[i + 1]));
        }
        if let Some((first, last)) = child_range {
            dict.set("First", Object::Reference(first));
            dict.set("Last", Object::Reference(last));
            // Positive count = open (descendants visible).
            dict.set("Count", Object::Integer(total_descendants(&item.children)));
        }

        doc.set_object(id, Object::Dictionary(dict));
    }

    Ok(Some((ids[0], *ids.last().unwrap())))
}

/// Build a `/Outlines` tree from `items` into `doc` and return the root
/// `/Outlines` dictionary's `ObjectId`. `page_ref` resolves a 0-based page index
/// to its page `ObjectId`; returning `None` is a hard error (invalid page).
///
/// An empty `items` yields a valid empty `/Outlines` (Count 0, no First/Last).
pub fn build_outline(
    doc: &mut Document,
    items: &[OutlineItem],
    page_ref: &dyn Fn(usize) -> Option<ObjectId>,
) -> Result<ObjectId, String> {
    let root_id = doc.new_object_id();

    let top = build_siblings(doc, items, root_id, page_ref)?;

    let mut root = Dictionary::new();
    root.set("Type", Object::Name(b"Outlines".to_vec()));
    root.set("Count", Object::Integer(total_descendants(items)));
    if let Some((first, last)) = top {
        root.set("First", Object::Reference(first));
        root.set("Last", Object::Reference(last));
    }
    doc.set_object(root_id, Object::Dictionary(root));

    Ok(root_id)
}

/// Apply an outline (parsed from `json`, a JSON array of `OutlineItem`) to a
/// loaded PDF `data` and return new PDF bytes (incremental update). Bookmark
/// destinations point at the existing pages by index.
pub fn set_outline_json(data: &[u8], json: &str) -> Result<Vec<u8>, String> {
    let items: Vec<OutlineItem> =
        serde_json::from_str(json).map_err(|e| format!("invalid outline json: {e}"))?;

    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let page_count = doc.get_pages().len();
    validate_pages(&items, page_count)?;

    // The Root reference in the trailer is the catalog object id.
    let root_catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;

    // Sorted-by-page-number page object ids of the previous document.
    let mut sorted_pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    sorted_pages.sort_by_key(|(num, _)| *num);

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    let page_ref = |i: usize| -> Option<ObjectId> { sorted_pages.get(i).map(|(_, id)| *id) };

    let root = build_outline(&mut inc.new_document, &items, &page_ref)?;

    // Clone the catalog into the new document and set /Outlines on it.
    inc.opt_clone_object_to_new_document(root_catalog_id)
        .map_err(|e| e.to_string())?;
    let catalog = inc
        .new_document
        .get_object_mut(root_catalog_id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    catalog.set("Outlines", Object::Reference(root));

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    const FICHA: &[u8] = include_bytes!(
        "../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf"
    );

    #[test]
    fn sets_outline_with_dest() {
        let out = set_outline_json(
            FICHA,
            r#"[{"title":"Intro","page":0},{"title":"End","page":0}]"#,
        )
        .unwrap();
        assert_eq!(&out[..FICHA.len()], FICHA); // incremental
        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        let outlines_ref = cat.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = doc.get_object(outlines_ref).unwrap().as_dict().unwrap();
        assert!(outlines.has(b"First") && outlines.has(b"Last"));
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert!(count >= 2);
    }

    #[test]
    fn nested_outline_links_parent() {
        let out = set_outline_json(
            FICHA,
            r#"[{"title":"Ch1","page":0,"children":[{"title":"1.1","page":0}]}]"#,
        )
        .unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let cat = doc.catalog().unwrap();
        let outlines = doc
            .get_object(cat.get(b"Outlines").unwrap().as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        let first = doc
            .get_object(outlines.get(b"First").unwrap().as_reference().unwrap())
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(first.has(b"First"), "nested item must have a child");
    }

    #[test]
    fn outline_rejects_bad_page() {
        assert!(set_outline_json(FICHA, r#"[{"title":"x","page":9999}]"#).is_err());
    }
}
