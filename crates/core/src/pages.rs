use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PageInfo {
    pub index: usize,
    pub width: f32,
    pub height: f32,
    pub rotation: i64,
}

/// Resolve a page attribute that may be inherited via /Parent (PDF 32000-1 7.7.3.4).
fn inherited<'a>(doc: &'a Document, page_id: lopdf::ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = page_id;
    for _ in 0..32 {
        let dict = doc.get_dictionary(current).ok()?;
        if let Ok(v) = dict.get(key) {
            return match v {
                Object::Reference(r) => doc.get_object(*r).ok(),
                other => Some(other),
            };
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(r)) => current = *r,
            _ => return None,
        }
    }
    None
}

fn rect_f32(arr: &[Object]) -> Option<[f32; 4]> {
    if arr.len() != 4 {
        return None;
    }
    let mut out = [0f32; 4];
    for (i, o) in arr.iter().enumerate() {
        out[i] = match o {
            Object::Integer(n) => *n as f32,
            Object::Real(n) => *n,
            _ => return None,
        };
    }
    Some(out)
}

/// Parse `data` and return its pages as a JSON array of `PageInfo` objects.
pub fn read_pages_json(data: &[u8]) -> Result<String, String> {
    let doc = Document::load_mem(data).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    for (i, (_, page_id)) in doc.get_pages().iter().enumerate() {
        let media = inherited(&doc, *page_id, b"MediaBox")
            .and_then(|o| o.as_array().ok())
            .and_then(|a| rect_f32(a))
            .ok_or_else(|| format!("page {i}: missing or invalid MediaBox"))?;
        let rotation = inherited(&doc, *page_id, b"Rotate")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        pages.push(PageInfo {
            index: i,
            width: (media[2] - media[0]).abs(),
            height: (media[3] - media[1]).abs(),
            rotation: rotation.rem_euclid(360),
        });
    }
    serde_json::to_string(&pages).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    #[test]
    fn reads_page_list() {
        let json = read_pages_json(FICHA).unwrap();
        let pages: Vec<PageInfo> = serde_json::from_str(&json).unwrap();
        assert!(!pages.is_empty());
        assert_eq!(pages[0].index, 0);
        assert!(pages[0].width > 100.0 && pages[0].height > 100.0);
        assert_eq!(pages[0].rotation % 90, 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_pages_json(b"not a pdf").is_err());
    }
}
