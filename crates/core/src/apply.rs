//! Batched save: apply fill, flatten, draw, metadata, and outline operations to
//! a loaded PDF in a single parse → mutate → serialize pass.
//!
//! The TypeScript `save()` pipeline historically chained one WASM call per
//! operation, each re-parsing and re-serializing the whole document (up to six
//! round-trips). `apply_all_json` loads the document once, runs every requested
//! operation's mutation core against a single `IncrementalDocument`, and saves
//! once.
//!
//! Each mutator follows the same shape: a Phase-A step reads the immutable
//! `doc` into a plan (so `doc` can move into the incremental document), and a
//! Phase-B step applies that plan to the shared `inc`. Because every mutator
//! reads base state via `inc.get_prev_documents()` and writes via idempotent
//! copy-on-write, sequencing them on one `inc` is equivalent to chaining them
//! over re-serialized bytes.
//!
//! Page-structure operations (insert/remove/move) are intentionally NOT handled
//! here: they rebuild the page tree, which would invalidate draw's page-index
//! resolution within the same pass. The TS layer falls back to the chained
//! pipeline when structure ops are queued.

use crate::{draw, fill, flatten, metadata, outline};
use lopdf::IncrementalDocument;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyPlan {
    #[serde(default)]
    fill: Option<Vec<fill::FillOp>>,
    #[serde(default)]
    flatten: Option<Vec<String>>,
    #[serde(default)]
    draw: Option<DrawPlan>,
    #[serde(default)]
    metadata: Option<metadata::Metadata>,
    #[serde(default)]
    outline: Option<Vec<outline::OutlineItem>>,
}

#[derive(Deserialize)]
struct DrawPlan {
    ops: Vec<draw::DrawOp>,
    #[serde(default)]
    fonts: Vec<draw::FontDesc>,
}

/// Apply every requested operation in `plan_json` to `data` in one pass.
///
/// `fill_images` carries the signature-image bytes referenced by fill ops;
/// `draw_images` carries the image/embedded-page bytes referenced by draw ops;
/// `fonts` carries the embedded-font bytes referenced by draw ops.
pub fn apply_all_json(
    data: &[u8],
    plan_json: &str,
    fill_images: &[u8],
    draw_images: &[u8],
    fonts: &[u8],
    compress: bool,
) -> Result<Vec<u8>, String> {
    let plan: ApplyPlan =
        serde_json::from_str(plan_json).map_err(|e| format!("invalid apply plan: {e}"))?;

    let doc = crate::doc_io::load_pdf(data)?;

    // Phase A — resolve everything that needs the immutable document, before it
    // is moved into the IncrementalDocument.
    let fill_plan = match &plan.fill {
        Some(ops) => Some(fill::fill_resolve(&doc, ops, fill_images)?),
        None => None,
    };
    let flatten_plan = match &plan.flatten {
        Some(names) => Some(flatten::flatten_resolve(&doc, names)?),
        None => None,
    };
    let meta_info = plan
        .metadata
        .as_ref()
        .map(|_| metadata::read_existing_info(&doc));
    let outline_prep = match &plan.outline {
        Some(items) => Some(outline::outline_prep(&doc, items)?),
        None => None,
    };

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);

    // Phase B — apply in the same order as the chained save() pipeline.
    if let Some(plan) = &fill_plan {
        fill::fill_apply(&mut inc, plan)?;
    }
    if let Some((field_ids, stamps)) = &flatten_plan {
        flatten::flatten_apply(&mut inc, field_ids, stamps)?;
    }
    if let Some(d) = &plan.draw {
        let used = draw::draw_used_chars(&d.ops);
        let built_fonts = {
            let mut add = |o| inc.new_document.add_object(o);
            draw::build_document_fonts(&mut add, &d.fonts, fonts, &used)?
        };
        draw::draw_apply(&mut inc, &d.ops, draw_images, fonts, &d.fonts, &built_fonts)?;
    }
    if let (Some(meta), Some(info)) = (&plan.metadata, meta_info) {
        metadata::metadata_apply(&mut inc, info, meta);
    }
    if let (Some(items), Some(prep)) = (&plan.outline, &outline_prep) {
        outline::outline_apply(&mut inc, items, prep)?;
    }

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Document, Object};

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

    fn page0_content(doc: &Document) -> String {
        let pages = doc.get_pages();
        let (_, page_id) = pages.into_iter().next().unwrap();
        let data = doc.get_page_content(page_id).unwrap();
        String::from_utf8_lossy(&data).into_owned()
    }

    fn info_title(doc: &Document) -> Option<String> {
        let id = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
        let info = doc.get_dictionary(id).ok()?;
        info.get(b"Title")
            .ok()?
            .as_str()
            .ok()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    #[test]
    fn apply_all_compresses_drawn_content_when_enabled() {
        // Repeat the drawn text so the incremental content stream is worth deflating.
        let plan = r#"{
            "draw": { "ops": [
                {"op":"text","page":0,"x":72,"y":72,"size":12,"font":"Helvetica","color":[0,0,0],"text":"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog."}
            ] }
        }"#;
        let compressed =
            apply_all_json(FICHA, plan, &[], &[], &[], true).expect("apply_all should succeed");
        let raw =
            apply_all_json(FICHA, plan, &[], &[], &[], false).expect("apply_all should succeed");
        assert!(
            compressed.len() < raw.len(),
            "compressed {} should be smaller than raw {}",
            compressed.len(),
            raw.len()
        );
    }

    #[test]
    fn apply_all_composes_draw_metadata_outline_in_one_pass() {
        let plan = r#"{
            "draw": { "ops": [
                {"op":"text","page":0,"x":72,"y":72,"size":12,"font":"Helvetica","color":[0,0,0],"text":"BPMERGE"}
            ] },
            "metadata": { "title": "Merged Title" },
            "outline": [ {"title":"Section","page":0} ]
        }"#;

        let out = apply_all_json(FICHA, plan, &[], &[], &[], false).expect("apply_all should succeed");
        let doc = Document::load_mem(&out).expect("output must be a valid PDF");

        // draw landed on page 0
        assert!(
            page0_content(&doc).contains("BPMERGE"),
            "drawn text missing from page content"
        );
        // metadata landed in the Info dict
        assert_eq!(info_title(&doc).as_deref(), Some("Merged Title"));
        // outline landed on the catalog
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        assert!(
            matches!(catalog.get(b"Outlines"), Ok(Object::Reference(_))),
            "catalog should reference an /Outlines tree"
        );
    }

    fn field_value(bytes: &[u8], name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).ok()?;
        let fields: serde_json::Value = serde_json::from_str(&json).ok()?;
        fields
            .as_array()?
            .iter()
            .find(|f| f["name"] == name)?
            .get("value")?
            .as_str()
            .map(|s| s.to_string())
    }

    #[test]
    fn apply_all_composes_fill_and_draw_on_same_page() {
        // Fill a text field AND draw text on page 0 in one pass: both must
        // survive (the page /Contents merge is the key composition risk).
        let plan = r#"{
            "fill": [ {"name":"beneficiario.apellidos_nombres","value":"FILLED"} ],
            "draw": { "ops": [
                {"op":"text","page":0,"x":72,"y":120,"size":12,"font":"Helvetica","color":[0,0,0],"text":"DRAWN"}
            ] }
        }"#;

        let out = apply_all_json(FICHA, plan, &[], &[], &[], false).expect("apply_all should succeed");
        let doc = Document::load_mem(&out).expect("output must be a valid PDF");

        assert!(
            page0_content(&doc).contains("DRAWN"),
            "drawn text missing after fill+draw merge"
        );
        assert_eq!(
            field_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("FILLED"),
            "filled field value missing after fill+draw merge"
        );
    }

    #[test]
    fn apply_all_empty_plan_roundtrips() {
        let out = apply_all_json(FICHA, "{}", &[], &[], &[], false).expect("empty plan should succeed");
        Document::load_mem(&out).expect("output must be a valid PDF");
    }

    #[test]
    fn apply_all_fill_then_flatten_stamps_filled_appearance() {
        // Regression: fill + flatten of the same field in ONE apply_all pass.
        // The flatten stamp must use the appearance generated by the fill in
        // this same pass, not the (absent) pre-fill appearance.
        let plan = r#"{
            "fill": [ {"name":"beneficiario.apellidos_nombres","value":"BATCHFLAT"} ],
            "flatten": ["beneficiario.apellidos_nombres"]
        }"#;
        let out = apply_all_json(FICHA, plan, &[], &[], &[], false).expect("apply_all should succeed");
        let doc = Document::load_mem(&out).expect("output must be a valid PDF");

        let content = page0_content(&doc);
        assert!(
            content.contains("/bpdfAp0 Do"),
            "flatten did not stamp the appearance into the page content"
        );

        let pages = doc.get_pages();
        let (_, page_id) = pages.into_iter().next().unwrap();
        let page = doc.get_dictionary(page_id).unwrap();
        let res = match page.get(b"Resources").unwrap() {
            Object::Reference(id) => doc.get_dictionary(*id).unwrap(),
            Object::Dictionary(d) => d,
            other => panic!("unexpected /Resources shape: {other:?}"),
        };
        let ap_id = res
            .get(b"XObject")
            .and_then(|o| o.as_dict())
            .expect("page resources must have /XObject")
            .get(b"bpdfAp0")
            .and_then(|o| o.as_reference())
            .expect("bpdfAp0 must be registered");
        let stream = doc.get_object(ap_id).unwrap().as_stream().unwrap();
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("BATCHFLAT"),
            "stamped appearance is not the one filled in this pass: {text}"
        );
    }

    #[test]
    fn shared_font_builds_once_across_draw_ops() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        // Created base doc with one page, then apply two draw ops using the same font.
        let base = crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]", "[]", false, false,
        ).unwrap();
        let plan = format!(
            r#"{{"draw":{{"ops":[
                {{"op":"text","page":0,"x":10,"y":40,"size":12,"text":"Ab","fontId":0,"color":[0,0,0]}},
                {{"op":"text","page":0,"x":10,"y":20,"size":12,"text":"Cd","fontId":0,"color":[0,0,0]}}
            ],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
            FONT.len()
        );
        let out = apply_all_json(&base, &plan, &[], &[], FONT, false).unwrap();
        let doc = lopdf::Document::load_mem(&out).unwrap();
        let type0_count = doc.objects.values().filter(|o| {
            o.as_dict().ok()
                .and_then(|d| d.get(b"Subtype").ok())
                .and_then(|s| s.as_name().ok())
                == Some(b"Type0")
        }).count();
        assert_eq!(type0_count, 1, "font must build exactly once");
    }

    #[test]
    fn apply_all_rejects_out_of_range_font_id() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let base = crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]", "[]", false, false,
        ).unwrap();
        // Only one FontDesc (id 0) is provided, but the op references fontId 5.
        let plan = format!(
            r#"{{"draw":{{"ops":[
                {{"op":"text","page":0,"x":10,"y":40,"size":12,"text":"Ab","fontId":5,"color":[0,0,0]}}
            ],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
            FONT.len()
        );
        let err = apply_all_json(&base, &plan, &[], &[], FONT, false)
            .expect_err("out-of-range fontId must return Err, not panic");
        assert!(
            err.contains("out of range"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn apply_all_rejects_font_desc_range_beyond_blob() {
        const FONT: &[u8] =
            include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");
        let base = crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#, &[], &[], "[]", "[]", false, false,
        ).unwrap();
        // FontDesc's offset+length exceeds the fonts blob length.
        let plan = format!(
            r#"{{"draw":{{"ops":[
                {{"op":"text","page":0,"x":10,"y":40,"size":12,"text":"Ab","fontId":0,"color":[0,0,0]}}
            ],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
            FONT.len() + 1000
        );
        let err = apply_all_json(&base, &plan, &[], &[], FONT, false)
            .expect_err("out-of-range font byte range must return Err, not panic");
        assert!(
            err.contains("out of range") || err.contains("out of bounds"),
            "unexpected error message: {err}"
        );
    }
}
