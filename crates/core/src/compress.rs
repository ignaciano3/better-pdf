use lopdf::Document;

/// Compress every generated stream in `doc` that permits compression and is
/// not already filtered. Delegates to lopdf's per-stream guard, so streams
/// with an existing `/Filter` (fonts, images) and streams that would not
/// shrink are left untouched. Idempotent.
pub fn compress_generated_streams(doc: &mut Document) {
    doc.compress();
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
        assert_eq!(stream.content, original, "filtered stream must be untouched");
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
        assert!(stream.dict.get(b"Filter").is_err(), "must stay uncompressed");
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
}
