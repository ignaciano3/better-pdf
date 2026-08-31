use lopdf::{Document, SaveOptions};

/// Compress every generated stream in `doc` that permits compression and is
/// not already filtered. Delegates to lopdf's per-stream guard, so streams
/// with an existing `/Filter` (fonts, images) and streams that would not
/// shrink are left untouched. Idempotent.
pub fn compress_generated_streams(doc: &mut Document) {
    doc.compress();
}

/// Serialize a freshly-built full `Document`, applying the two output-size
/// policies. `compress` deflates generated content/appearance/font stream
/// bodies (see `compress_generated_streams`). `object_streams` packs non-stream
/// objects into PDF object streams, which always imply cross-reference streams.
/// The two axes act on disjoint objects, so any combination is valid. Only
/// callable on a full `Document` — `IncrementalDocument` cannot emit object
/// streams.
pub fn serialize_document(
    doc: &mut Document,
    compress: bool,
    object_streams: bool,
) -> Result<Vec<u8>, String> {
    if compress {
        compress_generated_streams(doc);
    }
    let mut out = Vec::new();
    if object_streams {
        let options = SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build();
        doc.save_with_options(&mut out, options)
            .map_err(|e| e.to_string())?;
    } else {
        doc.save_to(&mut out).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Object, Stream, dictionary};

    fn highly_compressible() -> Vec<u8> {
        vec![b'A'; 4096]
    }

    #[test]
    fn compresses_unfiltered_stream() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            highly_compressible(),
        )));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert_eq!(
            stream.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert!(stream.content.len() < 4096);
    }

    #[test]
    fn skips_already_filtered_stream() {
        let mut doc = Document::with_version("1.7");
        let mut dict = dictionary! {};
        dict.set("Filter", "FlateDecode");
        let original = vec![1u8, 2, 3, 4];
        let id = doc.add_object(Object::Stream(Stream::new(dict, original.clone())));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert_eq!(
            stream.content, original,
            "filtered stream must be untouched"
        );
    }

    #[test]
    fn skips_stream_with_compression_disabled() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(
            Stream::new(dictionary! {}, highly_compressible()).with_compression(false),
        ));
        compress_generated_streams(&mut doc);
        let stream = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("expected stream"),
        };
        assert!(
            stream.dict.get(b"Filter").is_err(),
            "must stay uncompressed"
        );
    }

    #[test]
    fn idempotent() {
        let mut doc = Document::with_version("1.7");
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            highly_compressible(),
        )));
        compress_generated_streams(&mut doc);
        let after_first = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s.content.clone(),
            _ => panic!(),
        };
        compress_generated_streams(&mut doc);
        let after_second = match doc.objects.get(&id).unwrap() {
            Object::Stream(s) => s.content.clone(),
            _ => panic!(),
        };
        assert_eq!(after_first, after_second);
    }

    /// A valid `n`-page document: `n` page dicts + `n` content streams + a /Pages
    /// node + a /Catalog. Object streams pack the non-stream dicts (pages, catalog,
    /// pages-node); content streams stay direct.
    fn many_page_doc(n: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.add_object(dictionary! { "Type" => "Pages" });
        let mut kids = Vec::new();
        for _ in 0..n {
            let content = doc.add_object(Object::Stream(Stream::new(
                dictionary! {},
                b"BT ET".to_vec(),
            )));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Contents" => Object::Reference(content),
            });
            kids.push(Object::Reference(page));
        }
        let count = kids.len() as i64;
        if let Ok(p) = doc.get_object_mut(pages_id).and_then(Object::as_dict_mut) {
            p.set("Kids", Object::Array(kids));
            p.set("Count", Object::Integer(count));
        }
        let catalog = doc.add_object(
            dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) },
        );
        doc.trailer.set("Root", Object::Reference(catalog));
        doc
    }

    #[test]
    fn serialize_document_object_streams_packs_and_roundtrips() {
        let plain = serialize_document(&mut many_page_doc(40), false, false).unwrap();
        let packed = serialize_document(&mut many_page_doc(40), false, true).unwrap();

        // Object streams appear and shrink the object-heavy document.
        assert!(
            packed.windows(6).any(|w| w == b"ObjStm"),
            "expected an /ObjStm object stream in packed output"
        );
        assert!(
            packed.len() < plain.len(),
            "packed {} should be smaller than plain {}",
            packed.len(),
            plain.len()
        );

        // Packed output is a valid PDF that round-trips with all pages intact.
        let reloaded = Document::load_mem(&packed).unwrap();
        assert_eq!(reloaded.get_pages().len(), 40);
    }

    #[test]
    fn serialize_document_plain_has_no_object_stream() {
        let plain = serialize_document(&mut many_page_doc(5), true, false).unwrap();
        assert!(
            !plain.windows(6).any(|w| w == b"ObjStm"),
            "plain serialization must not emit object streams"
        );
        assert_eq!(Document::load_mem(&plain).unwrap().get_pages().len(), 5);
    }

    #[test]
    fn serialize_document_both_axes_produce_loadable_pdf() {
        // compress + object_streams together: disjoint object sets, must round-trip.
        let out = serialize_document(&mut many_page_doc(30), true, true).unwrap();
        assert!(
            out.windows(6).any(|w| w == b"ObjStm"),
            "expected an /ObjStm with both axes enabled"
        );
        assert_eq!(Document::load_mem(&out).unwrap().get_pages().len(), 30);
    }
}
