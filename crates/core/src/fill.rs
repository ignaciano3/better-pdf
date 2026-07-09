//! Fill engine: apply {name,value} ops to a PDF and incrementally save.

use crate::appearance;
use crate::draw::FontDesc;
use crate::fonts::{self, BuiltFont, MissingGlyphPolicy};
use crate::forms::{self};
use lopdf::{
    Dictionary, Document, IncrementalDocument, Object, ObjectId, decode_text_string, text_string,
};
use serde::Deserialize;
use std::collections::HashMap;

/// Everything `fill` needs to resolve/apply embedded-font (`fontId`) fills:
/// the font descriptors (byte ranges into `bytes`) and the already-built
/// Type0 object map (built once in `apply.rs`, before any fill fields are
/// touched, so both Phase A and Phase B can read it).
pub(crate) struct FontCtx<'a> {
    pub(crate) descs: &'a [FontDesc],
    pub(crate) built: &'a HashMap<usize, (ObjectId, BuiltFont)>,
    pub(crate) bytes: &'a [u8],
}

impl FontCtx<'_> {
    fn get(&self, id: usize) -> Result<(ObjectId, &BuiltFont, &[u8]), String> {
        if id >= self.descs.len() {
            return Err(format!("font id {id} out of range"));
        }
        let (type0_id, built) = self
            .built
            .get(&id)
            .ok_or_else(|| format!("font id {id} out of range"))?;
        let fd = &self.descs[id];
        let end = fd
            .offset
            .checked_add(fd.length)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        let bytes = self
            .bytes
            .get(fd.offset..end)
            .ok_or_else(|| "font range out of bounds".to_string())?;
        Ok((*type0_id, built, bytes))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FillOp {
    name: String,
    pub(crate) value: Option<String>,
    values: Option<Vec<String>>,
    /// When present, the op sets the field's default value (`/DV`) rather than
    /// its current value (`/V`). Mutually exclusive with `value`/`values`/image.
    pub(crate) default_value: Option<String>,
    /// When true, reset the field: set `/V` to the field's `/DV` (or clear it if
    /// there is none) and redraw. Mutually exclusive with the other op kinds.
    reset: Option<bool>,
    image_offset: Option<usize>,
    image_length: Option<usize>,
    /// When present, change the field's flags rather than its value. Mutually
    /// exclusive with every other op kind.
    flags: Option<FieldFlagOps>,
    /// Index into `plan.draw.fonts` (the same `FontDesc` list draw ops use).
    /// When present on a value-setting op, the appearance is rendered with the
    /// embedded (Type0/Identity-H) font instead of the WinAnsi engine.
    #[serde(default)]
    pub(crate) font_id: Option<usize>,
}

/// Flag mutations requested for one field. Each `Some(true)` sets the bit,
/// `Some(false)` clears it, and `None` leaves it untouched. `read_only`,
/// `required`, and `no_export` are field `/Ff` bits; `hidden`, `print`, and
/// `no_view` are annotation `/F` bits applied to every widget of the field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldFlagOps {
    read_only: Option<bool>,
    required: Option<bool>,
    no_export: Option<bool>,
    hidden: Option<bool>,
    print: Option<bool>,
    no_view: Option<bool>,
    /// Appearance-affecting text-field `/Ff` bits. Toggling any of these
    /// regenerates the field's appearance stream from its current `/V`.
    multiline: Option<bool>,
    password: Option<bool>,
    comb: Option<bool>,
    /// Cell count for the comb layout (written to `/MaxLen`). Required when
    /// turning `comb` on for a field that has no `/MaxLen`.
    comb_max_len: Option<i64>,
}

impl FieldFlagOps {
    /// True when the op touches a flag that changes how the value is drawn.
    fn touches_appearance(&self) -> bool {
        self.multiline.is_some() || self.password.is_some() || self.comb.is_some()
    }
}

/// Apply the given fill ops to `data` and return new PDF bytes (incremental
/// save). `images` is the concatenated image blob the ops' offsets index into.
pub fn fill_fields_json(
    data: &[u8],
    ops_json: &str,
    images: &[u8],
    compress: bool,
) -> Result<Vec<u8>, String> {
    let ops: Vec<FillOp> = serde_json::from_str(ops_json).map_err(|e| e.to_string())?;
    let doc = crate::doc_io::load_pdf(data)?;
    // The standalone fill path has no font blob / draw.fonts section, so
    // embedded-font fills (`fontId`) only flow through `apply_all_json`.
    let plan = fill_resolve(&doc, &ops, images, None)?;

    let mut inc = IncrementalDocument::create_from(data.to_vec(), doc);
    fill_apply(&mut inc, &plan, None)?;

    if compress {
        crate::compress::compress_generated_streams(&mut inc.new_document);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Phase A: resolve every fill op against the immutable `doc` into a plan, so
/// `doc` can be moved into the `IncrementalDocument` afterwards. Rejects XFA.
pub(crate) fn fill_resolve(
    doc: &Document,
    ops: &[FillOp],
    images: &[u8],
    font_ctx: Option<&FontCtx>,
) -> Result<Vec<Resolved>, String> {
    if forms::has_xfa(doc) {
        return Err(
            "XFA form detected: filling is not supported because viewers render the XFA data, not the AcroForm values"
                .to_string(),
        );
    }
    let mut plan: Vec<Resolved> = Vec::with_capacity(ops.len());
    for op in ops {
        plan.push(resolve(doc, op, images, font_ctx)?);
    }
    Ok(plan)
}

/// Phase B: apply a resolved fill plan to the incremental document.
pub(crate) fn fill_apply(
    inc: &mut IncrementalDocument,
    plan: &[Resolved],
    font_ctx: Option<&FontCtx>,
) -> Result<(), String> {
    let touched_appearance = plan.iter().any(|r| {
        matches!(
            r.apply,
            Apply::Text { .. }
                | Apply::Dropdown { .. }
                | Apply::ListBoxMulti { .. }
                | Apply::Signature { .. }
                | Apply::SetAppearanceFlags { .. }
        )
    });

    for r in plan {
        apply(inc, r, font_ctx)?;
    }
    if touched_appearance {
        clear_need_appearances(inc)?;
    }
    Ok(())
}

/// What to do to one field, pre-computed from the immutable document.
pub(crate) struct Resolved {
    field_id: ObjectId,
    apply: Apply,
}

/// A widget to draw an appearance on: its id and its /Rect [x0 y0 x1 y1].
struct WidgetBox {
    id: ObjectId,
    rect: [f32; 4],
}

/// Per-field appearance inputs shared by text and choice fields.
/// How to obtain the appearance stream's `/Resources/Font/<name>` reference.
enum FontRef {
    /// The DA font resolves to an existing `/DR/Font` object — reference it.
    Dr(ObjectId),
    /// The DA font names a standard-14 font that is absent from `/DR` (common
    /// in government forms whose `/DA` says `/Helvetica` but ship no `/DR`
    /// entry). Synthesize a Type1 font dict for the given /BaseFont at apply
    /// time and reference that, rather than failing the fill.
    Synth(&'static str),
}

struct ApInputs {
    da: appearance::Da,
    q: i64,
    font_ref: FontRef,
    font: String,
    widths: appearance::FontWidths,
    widgets: Vec<WidgetBox>,
    /// True for text-area fields (Ff Multiline bit); choice fields are always false.
    multiline: bool,
    /// True for comb text fields (Ff Comb bit): draw the value in fixed cells.
    comb: bool,
    /// True for password text fields (Ff Password bit): draw no visible value.
    password: bool,
    /// Cell count for the comb layout, from `/MaxLen`. Only meaningful when `comb`.
    max_len: i64,
    /// `Some(font_id)` when this field's appearance must be drawn with an
    /// embedded (Type0/Identity-H) font instead of the WinAnsi engine. The id
    /// indexes `plan.draw.fonts`; the actual `(ObjectId, BuiltFont)` + bytes
    /// are looked up at apply time via the threaded `FontCtx`.
    embedded: Option<usize>,
}

enum Apply {
    /// Set /V to a string literal and draw an appearance on each widget.
    Text { value: String, ap: ApInputs },
    /// Set /V (+ /I if matched) and draw an appearance on each widget.
    Dropdown {
        value: String,
        index: Option<i64>,
        ap: ApInputs,
    },
    /// Set group /V to a Name, and each widget's /AS (on-state name or "Off").
    Button {
        value: String,
        widgets: Vec<(ObjectId, bool)>,
    },
    /// Set the field's default value /DV only. `as_name` is true for button
    /// fields (checkbox/radio), where /DV is a Name; false for text/choice,
    /// where it is a text string. Does not draw or change any appearance.
    /// `embedded` mirrors `ApInputs::embedded`: `Some(font_id)` when this
    /// field's `/DA`/`/DR` must be rewired to the embedded font (same as the
    /// value path), and `/DV` must be written as UTF-16BE.
    DefaultValue {
        value: String,
        as_name: bool,
        embedded: Option<(usize, appearance::Da)>,
    },
    /// Draw a visual-only signature image appearance on each widget.
    Signature {
        image: appearance::SignatureImage,
        widgets: Vec<WidgetBox>,
    },
    /// Set /V to an array of strings and /I to the sorted array of indices,
    /// then draw a multi-row highlight appearance on each widget.
    ListBoxMulti {
        values: Vec<String>,
        indices: Vec<i64>,
        options: Vec<String>,
        ap: ApInputs,
    },
    /// Set/clear bits on the field's `/Ff` and on each widget's `/F`, without
    /// touching any value or appearance. `*_set`/`*_clear` are bit masks; the
    /// new value is `(current & !clear) | set`. `field_ff_base` is the field's
    /// effective (inherited) `/Ff` at resolve time, used when the field has no
    /// `/Ff` of its own so inherited bits are preserved.
    SetFlags {
        field_ff_set: i64,
        field_ff_clear: i64,
        field_ff_base: i64,
        widget_f_set: i64,
        widget_f_clear: i64,
        widgets: Vec<ObjectId>,
    },
    /// Toggle appearance-affecting text-field `/Ff` bits (multiline / comb /
    /// password) and redraw the field's appearance from `value` (its current
    /// `/V`). `max_len`, when present, is written to `/MaxLen` for comb layout.
    /// `ap` already reflects the *new* flag state.
    SetAppearanceFlags {
        field_ff_set: i64,
        field_ff_clear: i64,
        field_ff_base: i64,
        max_len: Option<i64>,
        value: String,
        ap: ApInputs,
    },
}

/// Locate the field for `op.name`, classify it, and dispatch to the branch that
/// handles this op's kind (flags / default-value / reset / value or image).
fn resolve(
    doc: &Document,
    op: &FillOp,
    images: &[u8],
    font_ctx: Option<&FontCtx>,
) -> Result<Resolved, String> {
    let (field_id, dict) =
        find_field(doc, &op.name).ok_or_else(|| format!("no such field: {}", op.name))?;
    let ft = forms::inherited_name(doc, dict, b"FT").unwrap_or_default();
    let ff = forms::inherited_int(doc, dict, b"Ff").unwrap_or(0);
    let kind = forms::classify(&ft, ff);

    if let Some(fid) = op.font_id {
        if kind != "text" || forms::is_comb(ff) {
            return Err(format!(
                "embedded fonts are supported on plain and multiline text fields only (field '{}')",
                op.name
            ));
        }
        match font_ctx {
            Some(ctx) if fid < ctx.descs.len() => {}
            _ => return Err(format!("font id {fid} out of range")),
        }
    }

    if let Some(flags) = &op.flags {
        return resolve_flags(doc, op, field_id, dict, kind, ff, flags);
    }
    if let Some(dv) = &op.default_value {
        return resolve_default_value(doc, op, field_id, dict, kind, dv, font_ctx);
    }
    if op.reset == Some(true) {
        return resolve_reset(doc, op, field_id, dict, kind, ff);
    }
    resolve_value(doc, op, images, field_id, dict, kind, ff, font_ctx)
}

/// Branch: change a field's flags (`/Ff` bits + per-widget `/F` bits). Mutually
/// exclusive with every value-bearing op kind. Appearance-affecting bits
/// (multiline / comb / password) take the redraw path via `SetAppearanceFlags`.
#[allow(clippy::too_many_arguments)]
fn resolve_flags(
    doc: &Document,
    op: &FillOp,
    field_id: ObjectId,
    dict: &Dictionary,
    kind: &str,
    ff: i64,
    flags: &FieldFlagOps,
) -> Result<Resolved, String> {
    // Changing flags is mutually exclusive with every value-bearing op kind.
    if op.value.is_some()
        || op.values.is_some()
        || op.default_value.is_some()
        || op.reset == Some(true)
        || op.image_offset.is_some()
    {
        return Err(format!(
            "field {} flags op cannot be combined with other mutations",
            op.name
        ));
    }
    let (mut field_set, mut field_clear) = (0i64, 0i64);
    flag_masks(flags.read_only, 1 << 0, &mut field_set, &mut field_clear);
    flag_masks(flags.required, 1 << 1, &mut field_set, &mut field_clear);
    flag_masks(flags.no_export, 1 << 2, &mut field_set, &mut field_clear);

    // Appearance-affecting flags (multiline/comb/password) require redrawing
    // the value, so they take a different path that regenerates the /AP.
    if flags.touches_appearance() {
        if kind != "text" {
            return Err(format!(
                "field {} flags multiline/comb/password apply only to text fields, not {}",
                op.name, kind
            ));
        }
        flag_masks(flags.multiline, 1 << 12, &mut field_set, &mut field_clear);
        flag_masks(flags.password, 1 << 13, &mut field_set, &mut field_clear);
        flag_masks(flags.comb, 1 << 24, &mut field_set, &mut field_clear);
        let new_ff = (ff & !field_clear) | field_set;

        // Resolve the comb cell count: explicit arg wins, else existing
        // /MaxLen. Turning comb on without either is an error.
        let mut write_max_len = None;
        if let Some(ml) = flags.comb_max_len {
            if ml < 0 {
                return Err(format!("field {} comb maxLen must be >= 0", op.name));
            }
            write_max_len = Some(ml);
        }
        if forms::is_comb(new_ff)
            && write_max_len.is_none()
            && forms::inherited_int(doc, dict, b"MaxLen").is_none()
        {
            return Err(format!(
                "field {} comb requires a maxLen (no /MaxLen present)",
                op.name
            ));
        }

        let value = read_v_string(doc, dict).unwrap_or_default();
        let mut ap = ap_inputs(doc, field_id, dict, &op.name, new_ff, None, None)?;
        if let Some(ml) = write_max_len {
            ap.max_len = ml;
        }
        return Ok(Resolved {
            field_id,
            apply: Apply::SetAppearanceFlags {
                field_ff_set: field_set,
                field_ff_clear: field_clear,
                field_ff_base: ff,
                max_len: write_max_len,
                value,
                ap,
            },
        });
    }

    let (mut widget_set, mut widget_clear) = (0i64, 0i64);
    flag_masks(flags.hidden, 1 << 1, &mut widget_set, &mut widget_clear);
    flag_masks(flags.print, 1 << 2, &mut widget_set, &mut widget_clear);
    flag_masks(flags.no_view, 1 << 5, &mut widget_set, &mut widget_clear);
    let widgets = if widget_set != 0 || widget_clear != 0 {
        widget_ids(field_id, dict)
    } else {
        Vec::new()
    };
    Ok(Resolved {
        field_id,
        apply: Apply::SetFlags {
            field_ff_set: field_set,
            field_ff_clear: field_clear,
            field_ff_base: ff,
            widget_f_set: widget_set,
            widget_f_clear: widget_clear,
            widgets,
        },
    })
}

/// Branch: set a field's default value (`/DV`) only. Mutually exclusive with
/// setting the current value/image, and only valid on value-bearing field types.
/// Does not draw or change any appearance.
fn resolve_default_value(
    doc: &Document,
    op: &FillOp,
    field_id: ObjectId,
    dict: &Dictionary,
    kind: &str,
    dv: &str,
    font_ctx: Option<&FontCtx>,
) -> Result<Resolved, String> {
    if op.value.is_some() || op.values.is_some() || op.image_offset.is_some() {
        return Err(format!(
            "field {} op cannot combine defaultValue with value/values/image",
            op.name
        ));
    }
    let as_name = match kind {
        "text" | "dropdown" | "listbox" => {
            if (kind == "dropdown" || kind == "listbox")
                && dv != "Off"
                && has_opt(doc, dict)
                && dropdown_index(doc, dict, dv).is_none()
            {
                return Err(format!("'{}' is not a valid option for {}", dv, op.name));
            }
            false
        }
        "checkbox" | "radio" => true,
        other => {
            return Err(format!(
                "cannot set default value on field {} of type {}",
                op.name, other
            ));
        }
    };
    let embedded = if let Some(fid) = op.font_id {
        // `resolve()` already validated kind == "text" (non-comb) and the
        // font id range for any op carrying `font_id`.
        let ctx = font_ctx.ok_or_else(|| format!("font id {fid} out of range"))?;
        let (_, built, _) = ctx.get(fid)?;
        fonts::gids_per_line(
            built,
            dv,
            MissingGlyphPolicy::Error,
            &format!("field '{}'", op.name),
        )?;
        let acro = forms::acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
        let da = appearance::parse_da(&effective_da(doc, dict, acro));
        Some((fid, da))
    } else {
        None
    };
    Ok(Resolved {
        field_id,
        apply: Apply::DefaultValue {
            value: dv.to_string(),
            as_name,
            embedded,
        },
    })
}

/// Branch: reset a field — set `/V` to the field's own `/DV` (or clear it when
/// there is none), redrawing the appearance. Trusts `/DV`, so option validation
/// is skipped.
fn resolve_reset(
    doc: &Document,
    op: &FillOp,
    field_id: ObjectId,
    dict: &Dictionary,
    kind: &str,
    ff: i64,
) -> Result<Resolved, String> {
    if op.value.is_some()
        || op.values.is_some()
        || op.default_value.is_some()
        || op.image_offset.is_some()
    {
        return Err(format!(
            "field {} reset op cannot be combined with other mutations",
            op.name
        ));
    }
    // Multi-select list boxes reset to their /DV array (or clear).
    if kind == "listbox" && forms::is_multiselect(ff) {
        let dv_values: Vec<String> = dict
            .get(b"DV")
            .ok()
            .map(|o| forms::resolve(doc, o))
            .and_then(|o| match o {
                Object::Array(a) => Some(
                    a.iter()
                        .filter_map(|e| read_object_string(doc, e))
                        .collect(),
                ),
                s @ Object::String(..) => decode_text_string(s).ok().map(|v| vec![v]),
                _ => None,
            })
            .unwrap_or_default();
        let options: Vec<String> = dict
            .get(b"Opt")
            .ok()
            .map(|o| forms::resolve(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().map(|e| forms::opt_export(doc, e)).collect())
            .unwrap_or_default();
        let mut pairs: Vec<(i64, String)> = Vec::new();
        for v in &dv_values {
            if let Some(i) = dropdown_index(doc, dict, v) {
                pairs.push((i, v.clone()));
            }
        }
        pairs.sort_unstable_by_key(|(i, _)| *i);
        let (indices, values): (Vec<i64>, Vec<String>) = pairs.into_iter().unzip();
        return Ok(Resolved {
            field_id,
            apply: Apply::ListBoxMulti {
                values,
                indices,
                options,
                ap: ap_inputs(doc, field_id, dict, &op.name, ff, None, None)?,
            },
        });
    }
    let value = read_dv_string(doc, dict).unwrap_or_else(|| match kind {
        "checkbox" | "radio" => "Off".to_string(),
        _ => String::new(),
    });
    let apply = value_apply(doc, field_id, dict, kind, ff, &op.name, &value, false, None, None)?;
    Ok(Resolved { field_id, apply })
}

/// Branch: set a field's current value (`/V`) from a single string, an array of
/// strings (multi-select list box), or a signature image.
#[allow(clippy::too_many_arguments)]
fn resolve_value(
    doc: &Document,
    op: &FillOp,
    images: &[u8],
    field_id: ObjectId,
    dict: &Dictionary,
    kind: &str,
    ff: i64,
    font_ctx: Option<&FontCtx>,
) -> Result<Resolved, String> {
    let image_bytes = match (op.image_offset, op.image_length) {
        (Some(off), Some(len)) => Some(
            off.checked_add(len)
                .and_then(|end| images.get(off..end))
                .ok_or_else(|| format!("image range out of bounds for field {}", op.name))?,
        ),
        (None, None) => None,
        _ => return Err(format!("field {} op has a partial image range", op.name)),
    };
    let apply = if let Some(image) = image_bytes {
        if op.value.is_some() {
            return Err(format!(
                "field {} op cannot contain both value and image",
                op.name
            ));
        }
        if kind != "signature" {
            return Err(format!(
                "cannot set image on field {} of type {}",
                op.name, kind
            ));
        }
        let image = appearance::signature_image(image)?;
        Apply::Signature {
            image,
            widgets: widget_boxes(doc, field_id, dict),
        }
    } else {
        // Multi-value fills are only legal on a multiselect list box.
        if let Some(values) = &op.values {
            if op.value.is_some() {
                return Err(format!(
                    "field {} op cannot contain both value and values",
                    op.name
                ));
            }
            if kind != "listbox" || !forms::is_multiselect(ff) {
                return Err(format!(
                    "field {} does not accept multiple values (not a multi-select list box)",
                    op.name
                ));
            }
            let options: Vec<String> = dict
                .get(b"Opt")
                .ok()
                .map(|o| forms::resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .map(|a| a.iter().map(|e| forms::opt_export(doc, e)).collect())
                .unwrap_or_default();
            // Build (index, value) pairs so /V and /I stay positionally aligned
            // after sorting by index (PDF §12.7.4.4 requires /V to match /I order).
            let mut pairs: Vec<(i64, String)> = Vec::with_capacity(values.len());
            for v in values {
                match dropdown_index(doc, dict, v) {
                    Some(i) => pairs.push((i, v.clone())),
                    None => {
                        return Err(format!("'{}' is not a valid option for {}", v, op.name));
                    }
                }
            }
            pairs.sort_unstable_by_key(|(i, _)| *i);
            let (indices, sorted_values): (Vec<i64>, Vec<String>) = pairs.into_iter().unzip();
            return Ok(Resolved {
                field_id,
                apply: Apply::ListBoxMulti {
                    values: sorted_values,
                    indices,
                    options,
                    ap: ap_inputs(doc, field_id, dict, &op.name, ff, None, None)?,
                },
            });
        }
        let value = op
            .value
            .as_ref()
            .ok_or_else(|| format!("missing value for field {}", op.name))?;
        value_apply(
            doc, field_id, dict, kind, ff, &op.name, value, true, op.font_id, font_ctx,
        )?
    };
    Ok(Resolved { field_id, apply })
}

/// Build the `Apply` for setting a single string `value` on a value-bearing
/// field. When `validate_option` is true, choice values are checked against the
/// field's options (the normal fill path); reset passes false because the value
/// comes from the field's own `/DV`. `font_id`/`font_ctx` are only meaningful
/// for `kind == "text"` (validated by the caller for every other kind).
#[allow(clippy::too_many_arguments)]
fn value_apply(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    kind: &str,
    ff: i64,
    name: &str,
    value: &str,
    validate_option: bool,
    font_id: Option<usize>,
    font_ctx: Option<&FontCtx>,
) -> Result<Apply, String> {
    Ok(match kind {
        "text" => {
            if let Some(fid) = font_id {
                // Range already validated by the caller; safe to unwrap here.
                let ctx = font_ctx.ok_or_else(|| format!("font id {fid} out of range"))?;
                let (_, built, _) = ctx.get(fid)?;
                fonts::gids_per_line(
                    built,
                    value,
                    MissingGlyphPolicy::Error,
                    &format!("field '{name}'"),
                )?;
            }
            Apply::Text {
                value: value.to_string(),
                ap: ap_inputs(doc, field_id, dict, name, ff, font_id, font_ctx)?,
            }
        }
        "checkbox" | "radio" => {
            // /Opt-indexed on-states only exist for radio groups (PDF §12.7.4.2.3);
            // checkboxes never carry /Opt, so gate the fallback to radio buttons —
            // matching the Ff radio-bit discriminator forms::classify uses.
            let is_radio = kind == "radio" && ff & (1 << 15) != 0;
            let (effective, widgets) = match button_widgets(doc, field_id, dict, value) {
                Ok(w) => (value.to_string(), w),
                Err(e) => match is_radio.then(|| opt_index_state(doc, dict, value)).flatten() {
                    Some(idx) => {
                        let widgets = button_widgets(doc, field_id, dict, &idx)?;
                        (idx, widgets)
                    }
                    None => return Err(e),
                },
            };
            Apply::Button {
                value: effective,
                widgets,
            }
        }
        "dropdown" | "listbox" => {
            let index = dropdown_index(doc, dict, value);
            if validate_option && value != "Off" && index.is_none() && has_opt(doc, dict) {
                return Err(format!("'{}' is not a valid option for {}", value, name));
            }
            Apply::Dropdown {
                value: value.to_string(),
                index,
                ap: ap_inputs(doc, field_id, dict, name, ff, None, None)?,
            }
        }
        other => return Err(format!("cannot fill field {} of type {}", name, other)),
    })
}

/// Read a field's `/DV` (default value) as a string. Button fields store it as a
/// Name; text/choice fields as a text string.
fn read_dv_string(doc: &Document, dict: &Dictionary) -> Option<String> {
    dict.get(b"DV")
        .ok()
        .and_then(|o| read_object_string(doc, o))
}

/// Extract a string from a Name or text-string object (e.g. a `/DV` array
/// element), dereferencing indirect references; returns `None` for other
/// object types.
fn read_object_string(doc: &Document, o: &Object) -> Option<String> {
    match forms::resolve(doc, o) {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        s @ Object::String(..) => decode_text_string(s).ok(),
        _ => None,
    }
}

/// Gather everything needed to draw a text/choice field's appearance:
/// effective DA, quadding, the DR font reference, and the widget boxes.
fn ap_inputs(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    name: &str,
    ff: i64,
    font_id: Option<usize>,
    font_ctx: Option<&FontCtx>,
) -> Result<ApInputs, String> {
    let acro = forms::acroform(doc).ok_or_else(|| "no AcroForm".to_string())?;
    let da_str = effective_da(doc, dict, acro);
    let da = appearance::parse_da(&da_str);

    if let Some(fid) = font_id {
        // Embedded path: the DA font resolution below is entirely bypassed —
        // the field's /DA and appearance will reference /BPF<fid> instead.
        let ctx = font_ctx.ok_or_else(|| format!("font id {fid} out of range"))?;
        ctx.get(fid)?; // validates range; the actual lookup happens at apply time.
        return Ok(ApInputs {
            q: quadding(doc, dict),
            font: String::new(),
            widths: appearance::helvetica_widths(),
            da,
            font_ref: FontRef::Dr((0, 0)), // unused for embedded fields
            widgets: widget_boxes(doc, field_id, dict),
            multiline: forms::is_multiline(ff),
            comb: forms::is_comb(ff),
            password: forms::is_password(ff),
            max_len: forms::inherited_int(doc, dict, b"MaxLen").unwrap_or(0),
            embedded: Some(fid),
        });
    }

    // Reject Type0 (embedded/composite) DA fonts: the WinAnsi engine below would
    // mis-encode them. The caller must pass `{ font }` to `setText` so this
    // field's fill goes through the embedded path above instead.
    if let Some(fd) = font_dict(doc, acro, &da.font)
        && matches!(
            fd.get(b"Subtype").ok().and_then(|o| o.as_name().ok()),
            Some(b"Type0")
        )
    {
        return Err(format!(
            "field '{name}' uses an embedded font; pass {{ font }} to setText with an embedded font"
        ));
    }
    // Resolve the DA font to a /DR object; when it is absent but names a
    // standard-14 font, synthesize the font dict at apply time instead of
    // failing (real government forms often reference /Helvetica with no /DR).
    let font_ref = match font_ref(doc, acro, &da.font) {
        Some(id) => FontRef::Dr(id),
        None => match da_font_base(&da.font) {
            Some(base) => FontRef::Synth(base),
            None => {
                return Err(format!("DA font '{}' not found in /DR for {}", da.font, name));
            }
        },
    };
    let widths = match font_ref {
        FontRef::Synth(base) => {
            appearance::standard_14_widths(base).unwrap_or_else(appearance::helvetica_widths)
        }
        FontRef::Dr(_) => resolve_widths(doc, acro, &da.font),
    };
    Ok(ApInputs {
        q: quadding(doc, dict),
        font: da.font.clone(),
        widths,
        da,
        font_ref,
        widgets: widget_boxes(doc, field_id, dict),
        multiline: forms::is_multiline(ff),
        comb: forms::is_comb(ff),
        password: forms::is_password(ff),
        max_len: forms::inherited_int(doc, dict, b"MaxLen").unwrap_or(0),
        embedded: None,
    })
}

/// Read a field's `/V` (current value) as a string, if present.
fn read_v_string(doc: &Document, dict: &Dictionary) -> Option<String> {
    dict.get(b"V")
        .ok()
        .map(|o| forms::resolve(doc, o))
        .and_then(|o| decode_text_string(o).ok())
}

/// Effective /DA: field's own, else inherited, else AcroForm's, else default.
fn effective_da(doc: &Document, dict: &Dictionary, acro: &Dictionary) -> String {
    if let Some(s) = forms::inherited_str(doc, dict, b"DA") {
        return s;
    }
    acro.get(b"DA")
        .ok()
        .and_then(forms::da_string)
        .unwrap_or_else(|| "/Helv 0 Tf 0 g".to_string())
}

/// Resolve `font` (from DA) to its indirect object id via AcroForm /DR/Font.
/// The raw `/DR/Font/<name>` entry (a reference or inline dict) for a DA font.
fn dr_font_entry<'a>(doc: &'a Document, acro: &'a Dictionary, font: &str) -> Option<&'a Object> {
    let dr = forms::as_dict(doc, acro.get(b"DR").ok()?).ok()?;
    let fonts = forms::as_dict(doc, dr.get(b"Font").ok()?).ok()?;
    fonts.get(font.as_bytes()).ok()
}

fn font_ref(doc: &Document, acro: &Dictionary, font: &str) -> Option<ObjectId> {
    dr_font_entry(doc, acro, font)?.as_reference().ok()
}

/// Map a DA font resource name to a standard-14 `/BaseFont`, accepting both the
/// canonical PostScript names (`Helvetica`, `Times-Roman`, …) and the
/// conventional AcroForm aliases (`Helv`, `TiRo`, …). Used to synthesize a
/// font dict when the DA font is absent from `/DR`. Mirrors the alias set of
/// `create::da_font_alias`; Symbol/ZapfDingbats are intentionally excluded
/// (their custom encodings don't fit the WinAnsi text engine).
fn da_font_base(name: &str) -> Option<&'static str> {
    Some(match name {
        "Helvetica" | "Helv" => "Helvetica",
        "Helvetica-Bold" | "HeBo" => "Helvetica-Bold",
        "Helvetica-Oblique" | "HeOb" => "Helvetica-Oblique",
        "Helvetica-BoldOblique" | "HeBO" => "Helvetica-BoldOblique",
        "Courier" | "Cour" => "Courier",
        "Courier-Bold" | "CoBo" => "Courier-Bold",
        "Courier-Oblique" | "CoOb" => "Courier-Oblique",
        "Courier-BoldOblique" | "CoBO" => "Courier-BoldOblique",
        "Times-Roman" | "TiRo" => "Times-Roman",
        "Times-Bold" | "TiBo" => "Times-Bold",
        "Times-Italic" | "TiIt" => "Times-Italic",
        "Times-BoldItalic" | "TiBI" => "Times-BoldItalic",
        _ => return None,
    })
}

/// Resolve an `ApInputs` font reference to a concrete `/DR`-style object id in
/// the incremental document, synthesizing a standard-14 Type1 font dict when
/// the DA font was absent from `/DR` (`FontRef::Synth`).
fn resolve_font_ref(inc: &mut IncrementalDocument, font_ref: &FontRef) -> ObjectId {
    match font_ref {
        FontRef::Dr(id) => *id,
        FontRef::Synth(base) => inc
            .new_document
            .add_object(Object::Dictionary(crate::draw::font_dict(base))),
    }
}

/// Collect a field's drawable widgets (id + /Rect). A field with no /Kids is
/// its own widget.
fn widget_boxes(doc: &Document, field_id: ObjectId, dict: &Dictionary) -> Vec<WidgetBox> {
    widget_ids(field_id, dict)
        .into_iter()
        .filter_map(|id| {
            let d = doc.get_dictionary(id).ok()?;
            let r = d.get(b"Rect").ok()?.as_array().ok()?;
            let mut rect = [0f32; 4];
            for (i, v) in r.iter().enumerate().take(4) {
                rect[i] = v.as_float().unwrap_or(0.0);
            }
            Some(WidgetBox { id, rect })
        })
        .collect()
}

fn quadding(doc: &Document, dict: &Dictionary) -> i64 {
    forms::inherited_int(doc, dict, b"Q").unwrap_or(0)
}

/// Fold one tri-state flag request into set/clear bit masks: `Some(true)` sets
/// the bit, `Some(false)` clears it, `None` leaves it alone.
fn flag_masks(opt: Option<bool>, bit: i64, set: &mut i64, clear: &mut i64) {
    match opt {
        Some(true) => *set |= bit,
        Some(false) => *clear |= bit,
        None => {}
    }
}

/// Collect a field's widget annotation ids: its /Kids, or the field itself when
/// it has none (the common single-widget case).
fn widget_ids(field_id: ObjectId, dict: &Dictionary) -> Vec<ObjectId> {
    let ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    if ids.is_empty() { vec![field_id] } else { ids }
}

/// The /DR/Font/<name> dictionary for a DA font name, if present.
fn font_dict<'a>(doc: &'a Document, acro: &'a Dictionary, font: &str) -> Option<&'a Dictionary> {
    forms::as_dict(doc, dr_font_entry(doc, acro, font)?).ok()
}

/// Width table for the DA font: standard-14 metrics by /BaseFont when
/// recognized, else the font's own /Widths array, else Helvetica.
fn resolve_widths(doc: &Document, acro: &Dictionary, da_font: &str) -> appearance::FontWidths {
    if let Some(fd) = font_dict(doc, acro, da_font) {
        if let Some(base) = fd.get(b"BaseFont").ok().and_then(|o| o.as_name().ok())
            && let Some(w) = appearance::standard_14_widths(&String::from_utf8_lossy(base))
        {
            return w;
        }
        if let Some(w) = widths_from_font_dict(doc, fd) {
            return w;
        }
    }
    appearance::helvetica_widths()
}

/// Build a width table from a simple font's /FirstChar + /Widths entries.
fn widths_from_font_dict(doc: &Document, fd: &Dictionary) -> Option<appearance::FontWidths> {
    let first = fd.get(b"FirstChar").ok()?.as_i64().ok()?;
    let widths_obj = fd.get(b"Widths").ok()?;
    let arr = match widths_obj {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
        Object::Array(a) => a,
        _ => return None,
    };
    let mut table = [0u16; 224];
    for (i, w) in arr.iter().enumerate() {
        let code = first + i as i64;
        if (32..=255).contains(&code) {
            table[(code - 32) as usize] = w.as_float().unwrap_or(0.0).round() as u16;
        }
    }
    Some(appearance::FontWidths(table))
}

/// Resolve the button's widget set and validate the requested on-state.
/// Returns (widget_id, has_target_state) for each widget. A field with no
/// /Kids is its own widget.
fn button_widgets(
    doc: &Document,
    field_id: ObjectId,
    dict: &Dictionary,
    value: &str,
) -> Result<Vec<(ObjectId, bool)>, String> {
    let mut widgets: Vec<(ObjectId, bool)> = Vec::new();
    let kid_ids: Vec<ObjectId> = dict
        .get(b"Kids")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let targets: Vec<ObjectId> = if kid_ids.is_empty() {
        vec![field_id]
    } else {
        kid_ids
    };

    let mut any_match = false;
    for id in targets {
        let has = doc
            .get_dictionary(id)
            .ok()
            .map(|w| widget_has_state(doc, w, value))
            .unwrap_or(false);
        if has {
            any_match = true;
        }
        widgets.push((id, has));
    }
    if value != "Off" && !any_match {
        return Err(format!(
            "'{}' is not a valid on-state for this button",
            value
        ));
    }
    Ok(widgets)
}

/// True if a widget's /AP/N has a sub-key named `state`.
fn widget_has_state(doc: &Document, widget: &Dictionary, state: &str) -> bool {
    let mut found = Vec::new();
    forms::collect_on_states(doc, widget, &mut found);
    found.iter().any(|s| s == state)
}

fn has_opt(doc: &Document, dict: &Dictionary) -> bool {
    dict.get(b"Opt")
        .ok()
        .map(|o| forms::resolve(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

/// Index of `value` within /Opt (matching export value), if present.
fn dropdown_index(doc: &Document, dict: &Dictionary, value: &str) -> Option<i64> {
    let arr = forms::resolve(doc, dict.get(b"Opt").ok()?).as_array().ok()?;
    arr.iter()
        .position(|o| forms::opt_export(doc, o) == value)
        .map(|i| i as i64)
}

/// When a radio group carries /Opt, its on-states are indices; translate an
/// /Opt label to its index state ("Marcus Aurelius 🏛️" -> "0").
fn opt_index_state(doc: &Document, dict: &Dictionary, label: &str) -> Option<String> {
    let arr = forms::resolve(doc, dict.get(b"Opt").ok()?).as_array().ok()?;
    arr.iter()
        .position(|o| forms::opt_export(doc, o) == label)
        .map(|i| i.to_string())
}

/// Walk /AcroForm/Fields (and /Kids) to find the field whose fully-qualified
/// name equals `name`. Only reference-addressable fields are considered.
pub(crate) fn find_field<'a>(doc: &'a Document, name: &str) -> Option<(ObjectId, &'a Dictionary)> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    let acro = forms::as_dict(doc, catalog.get(b"AcroForm").ok()?).ok()?;
    let entries = acro.get(b"Fields").ok()?.as_array().ok()?;
    let mut stack: Vec<ObjectId> = entries
        .iter()
        .filter_map(|e| e.as_reference().ok())
        .collect();
    let mut seen = 0usize;
    while let Some(id) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break; // guard against pathological/cyclic field trees
        }
        let Ok(d) = doc.get_dictionary(id) else {
            continue;
        };
        if forms::fully_qualified_name(doc, d) == name {
            return Some((id, d));
        }
        if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
            for k in kids {
                if let Ok(kid_id) = k.as_reference() {
                    stack.push(kid_id);
                }
            }
        }
    }
    // Fallback: an orphaned widget field — a Widget annotation with its own /T
    // that was never linked into /AcroForm/Fields (see forms::append_orphan_widget_fields).
    // Match it on the page /Annots by fully-qualified name so it stays fillable.
    find_orphan_widget_field(doc, name)
}

/// Scan page `/Annots` for a Widget annotation whose fully-qualified name is
/// `name` and that carries its own `/T` (a terminal field), for fields that
/// exist only on the page and not in `/AcroForm/Fields`.
fn find_orphan_widget_field<'a>(
    doc: &'a Document,
    name: &str,
) -> Option<(ObjectId, &'a Dictionary)> {
    for (_, &pid) in doc.get_pages().iter() {
        let page = doc.get_dictionary(pid).ok()?;
        let Some(annots) = page
            .get(b"Annots")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_array().ok())
        else {
            continue;
        };
        for a in annots {
            let Ok(id) = a.as_reference() else { continue };
            let Ok(d) = doc.get_dictionary(id) else { continue };
            if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Widget")
                && d.has(b"T")
                && forms::fully_qualified_name(doc, d) == name
            {
                return Some((id, d));
            }
        }
    }
    None
}

/// Always encode as UTF-16BE (with BOM), regardless of ASCII-ness. Used for
/// embedded-font fields, where the value may need to round-trip through a
/// glyph range PDFDocEncoding can't represent even when the current value
/// happens to be pure ASCII.
fn embedded_text_string(value: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF]; // BOM
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

/// Apply one resolved mutation onto the incremental document.
fn apply(
    inc: &mut IncrementalDocument,
    r: &Resolved,
    font_ctx: Option<&FontCtx>,
) -> Result<(), String> {
    inc.opt_clone_object_to_new_document(r.field_id)
        .map_err(|e| e.to_string())?;
    match &r.apply {
        Apply::Text { value, ap } => {
            let v = if ap.embedded.is_some() {
                embedded_text_string(value)
            } else {
                text_string(value)
            };
            field_dict_mut(inc, r.field_id)?.set("V", v);
            if let Some(fid) = ap.embedded {
                let ctx = font_ctx
                    .ok_or_else(|| "internal: embedded field missing font context".to_string())?;
                let (type0_id, _, _) = ctx.get(fid)?;
                write_embedded_da(inc, r.field_id, fid, &ap.da)?;
                wire_dr_font(inc, &format!("BPF{fid}"), type0_id)?;
            }
            draw_appearances(inc, value, ap, font_ctx)?;
        }
        Apply::Dropdown { value, index, ap } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                d.set("V", text_string(value));
                match index {
                    Some(i) => {
                        d.set("I", Object::Array(vec![Object::Integer(*i)]));
                    }
                    None => {
                        d.remove(b"I");
                    }
                }
            }
            draw_appearances(inc, value, ap, None)?;
        }
        Apply::Button { value, widgets } => {
            field_dict_mut(inc, r.field_id)?.set("V", Object::Name(value.as_bytes().to_vec()));
            for (wid, has) in widgets {
                inc.opt_clone_object_to_new_document(*wid)
                    .map_err(|e| e.to_string())?;
                let as_state = if value != "Off" && *has {
                    value.as_str()
                } else {
                    "Off"
                };
                field_dict_mut(inc, *wid)?.set("AS", Object::Name(as_state.as_bytes().to_vec()));
            }
        }
        Apply::Signature { image, widgets } => {
            draw_signature_appearances(inc, image, widgets)?;
        }
        Apply::DefaultValue {
            value,
            as_name,
            embedded,
        } => {
            let dv = if *as_name {
                Object::Name(value.as_bytes().to_vec())
            } else if embedded.is_some() {
                embedded_text_string(value)
            } else {
                text_string(value)
            };
            field_dict_mut(inc, r.field_id)?.set("DV", dv);
            if let Some((fid, da)) = embedded {
                let ctx = font_ctx
                    .ok_or_else(|| "internal: embedded field missing font context".to_string())?;
                let (type0_id, _, _) = ctx.get(*fid)?;
                write_embedded_da(inc, r.field_id, *fid, da)?;
                wire_dr_font(inc, &format!("BPF{fid}"), type0_id)?;
            }
        }
        Apply::ListBoxMulti {
            values,
            indices,
            options,
            ap,
        } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                let v_arr: Vec<Object> = values.iter().map(|s| text_string(s)).collect();
                d.set("V", Object::Array(v_arr));
                let i_arr: Vec<Object> = indices.iter().map(|i| Object::Integer(*i)).collect();
                d.set("I", Object::Array(i_arr));
            }
            draw_listbox_multi_appearances(inc, options, indices, ap)?;
        }
        Apply::SetFlags {
            field_ff_set,
            field_ff_clear,
            field_ff_base,
            widget_f_set,
            widget_f_clear,
            widgets,
        } => {
            if *field_ff_set != 0 || *field_ff_clear != 0 {
                let d = field_dict_mut(inc, r.field_id)?;
                // Prefer the field's own /Ff (which may already reflect an
                // earlier op this run); fall back to the inherited base so we
                // don't drop inherited bits when the field has no /Ff of its own.
                let current = d
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(*field_ff_base);
                let next = (current & !*field_ff_clear) | *field_ff_set;
                d.set("Ff", Object::Integer(next));
            }
            for wid in widgets {
                inc.opt_clone_object_to_new_document(*wid)
                    .map_err(|e| e.to_string())?;
                let d = field_dict_mut(inc, *wid)?;
                let current = d.get(b"F").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
                let next = (current & !*widget_f_clear) | *widget_f_set;
                d.set("F", Object::Integer(next));
            }
        }
        Apply::SetAppearanceFlags {
            field_ff_set,
            field_ff_clear,
            field_ff_base,
            max_len,
            value,
            ap,
        } => {
            {
                let d = field_dict_mut(inc, r.field_id)?;
                let current = d
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(*field_ff_base);
                let next = (current & !*field_ff_clear) | *field_ff_set;
                d.set("Ff", Object::Integer(next));
                if let Some(ml) = max_len {
                    d.set("MaxLen", Object::Integer(*ml));
                }
            }
            draw_appearances(inc, value, ap, None)?;
        }
    }
    Ok(())
}

fn field_dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.new_document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())
}

/// Build and attach a `/AP/N` appearance stream on each of the field's widgets.
fn draw_appearances(
    inc: &mut IncrementalDocument,
    value: &str,
    ap: &ApInputs,
    font_ctx: Option<&FontCtx>,
) -> Result<(), String> {
    if let Some(fid) = ap.embedded {
        return draw_embedded_appearances(inc, value, ap, fid, font_ctx);
    }
    let text = appearance::encode_winansi(value);
    // Resolve once and share the (possibly synthesized) font object across all
    // of the field's widgets.
    let font_ref = resolve_font_ref(inc, &ap.font_ref);
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content = if ap.password {
            // Password fields never render their value into the appearance
            // stream (the value would otherwise leak in plain text).
            appearance::text_appearance_content_empty()
        } else if ap.comb {
            let size = appearance::auto_size(ap.da.size, &text, (w - 4.0).max(1.0), h, &ap.widths);
            appearance::text_appearance_content_comb(
                &text,
                size,
                w,
                h,
                ap.max_len,
                &ap.da.color,
                &ap.font,
                &ap.widths,
            )
        } else if ap.multiline {
            // Multiline: do not shrink-to-fit width (we wrap instead). Honor an
            // explicit DA size; for auto (size 0) use a fixed, height-clamped
            // default so wrapping has a stable measure.
            let size = if ap.da.size > 0.0 {
                ap.da.size
            } else {
                (h - 2.0).clamp(appearance::MIN_AUTO, appearance::MAX_AUTO)
            };
            let avail_w = (w - 4.0).max(1.0);
            let lines = appearance::wrap_lines(&text, size, avail_w, &ap.widths);
            appearance::text_appearance_content_multiline(
                &lines,
                size,
                w,
                h,
                ap.q,
                &ap.da.color,
                &ap.font,
                &ap.widths,
            )
        } else {
            let size = appearance::auto_size(ap.da.size, &text, (w - 4.0).max(1.0), h, &ap.widths);
            appearance::text_appearance_content(
                &text,
                size,
                w,
                h,
                ap.q,
                &ap.da.color,
                &ap.font,
                &ap.widths,
            )
        };
        let xobj = appearance::build_appearance_xobject(content, w, h, &ap.font, font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Embedded-font sibling of the WinAnsi branch in `draw_appearances`: draws
/// each widget's appearance with the Type0/Identity-H font built for `fid`,
/// aliased `/BPF<fid>` (matching the field's `/DA`, wired via `write_embedded_da`).
fn draw_embedded_appearances(
    inc: &mut IncrementalDocument,
    value: &str,
    ap: &ApInputs,
    fid: usize,
    font_ctx: Option<&FontCtx>,
) -> Result<(), String> {
    let ctx =
        font_ctx.ok_or_else(|| "internal: embedded field missing font context".to_string())?;
    let (type0_id, built, bytes) = ctx.get(fid)?;
    let alias = format!("BPF{fid}");
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content: Vec<u8> = if ap.password {
            appearance::text_appearance_content_empty()
        } else if ap.multiline {
            let size = if ap.da.size > 0.0 {
                ap.da.size
            } else {
                (h - 2.0).clamp(appearance::MIN_AUTO, appearance::MAX_AUTO)
            };
            let avail_w = (w - 4.0).max(1.0);
            let wrapped = fonts::wrap_embedded(bytes, size, avail_w, value)?;
            let lines: Vec<&str> = wrapped.split('\n').collect();
            appearance::text_appearance_content_embedded_multiline(
                &lines,
                size,
                w,
                h,
                ap.q,
                &ap.da.color,
                &alias,
                built,
                bytes,
            )?
            .into_bytes()
        } else {
            let avail_w = (w - 4.0).max(1.0);
            let size = if ap.da.size > 0.0 {
                ap.da.size
            } else {
                let base = (h - 2.0).clamp(appearance::MIN_AUTO, appearance::MAX_AUTO);
                let tw = fonts::measure_embedded(bytes, base, value).unwrap_or(0.0);
                if tw > avail_w && tw > 0.0 {
                    (base * avail_w / tw).max(appearance::MIN_AUTO)
                } else {
                    base
                }
            };
            appearance::text_appearance_content_embedded(
                value, size, w, h, ap.q, &ap.da.color, &alias, built, bytes,
            )
        };
        let xobj = appearance::build_appearance_xobject(content, w, h, &alias, type0_id);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Set the field's `/DA` to reference its embedded font's `/BPF<fid>` alias.
fn write_embedded_da(
    inc: &mut IncrementalDocument,
    field_id: ObjectId,
    fid: usize,
    da: &appearance::Da,
) -> Result<(), String> {
    let d = field_dict_mut(inc, field_id)?;
    d.set(
        "DA",
        Object::string_literal(format!("/BPF{fid} {} Tf {}", da.size, da.color)),
    );
    Ok(())
}

/// Ensure the AcroForm's `/DR/Font` has `alias -> type0_id`, creating `/DR`
/// and/or `/DR/Font` if the loaded doc lacks them. Mirrors the cloning
/// pattern `clear_need_appearances` uses to reach whichever object holds the
/// AcroForm (the Catalog inline, or the AcroForm's own indirect object).
fn wire_dr_font(inc: &mut IncrementalDocument, alias: &str, type0_id: ObjectId) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let root = prev
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            ensure_dr_font(inc, id, alias, type0_id)?;
        }
        Ok(Object::Dictionary(_)) => {
            inc.opt_clone_object_to_new_document(root)
                .map_err(|e| e.to_string())?;
            ensure_dr_font(inc, root, alias, type0_id)?;
        }
        _ => {}
    }
    Ok(())
}

/// Set `alias -> type0_id` in the `/DR/Font` reachable from `acro_holder_id`
/// (the object holding the `AcroForm` dict inline - either the Catalog or
/// the AcroForm's own indirect object, already cloned into the new
/// document). Creates `/DR` and/or `/DR/Font` if absent. Both `/DR` and
/// `/DR/Font` may themselves be indirect references (common in
/// Acrobat-authored PDFs) - in that case the referenced object is resolved
/// from the previous document, cloned into the new document (preserving its
/// existing entries), and overridden there via the incremental-update object
/// id, rather than being replaced with a fresh dict that would discard any
/// pre-existing fonts (e.g. `/Helv`).
fn ensure_dr_font(
    inc: &mut IncrementalDocument,
    acro_holder_id: ObjectId,
    alias: &str,
    type0_id: ObjectId,
) -> Result<(), String> {
    let dr_entry = field_dict_mut(inc, acro_holder_id)?
        .get(b"DR")
        .ok()
        .cloned();
    match dr_entry {
        Some(Object::Reference(dr_id)) => {
            inc.opt_clone_object_to_new_document(dr_id)
                .map_err(|e| e.to_string())?;
            ensure_font_dict(inc, dr_id, alias, type0_id)
        }
        Some(Object::Dictionary(_)) => {
            let font_entry = field_dict_mut(inc, acro_holder_id)?
                .get_mut(b"DR")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?
                .get(b"Font")
                .ok()
                .cloned();
            match font_entry {
                Some(Object::Reference(font_id)) => {
                    inc.opt_clone_object_to_new_document(font_id)
                        .map_err(|e| e.to_string())?;
                    field_dict_mut(inc, font_id)?
                        .set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
                }
                Some(Object::Dictionary(_)) => {
                    let dr = field_dict_mut(inc, acro_holder_id)?
                        .get_mut(b"DR")
                        .and_then(Object::as_dict_mut)
                        .map_err(|e| e.to_string())?;
                    let fonts = dr
                        .get_mut(b"Font")
                        .and_then(Object::as_dict_mut)
                        .map_err(|e| e.to_string())?;
                    fonts.set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
                }
                _ => {
                    let mut fonts = Dictionary::new();
                    fonts.set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
                    let dr = field_dict_mut(inc, acro_holder_id)?
                        .get_mut(b"DR")
                        .and_then(Object::as_dict_mut)
                        .map_err(|e| e.to_string())?;
                    dr.set("Font", Object::Dictionary(fonts));
                }
            }
            Ok(())
        }
        _ => {
            let mut fonts = Dictionary::new();
            fonts.set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
            let mut dr = Dictionary::new();
            dr.set("Font", Object::Dictionary(fonts));
            field_dict_mut(inc, acro_holder_id)?.set("DR", Object::Dictionary(dr));
            Ok(())
        }
    }
}

/// Set `alias -> type0_id` in the `/Font` dict reachable from `dr_id` (an
/// object holding a `DR`-shaped dict, already cloned into the new
/// document), resolving `/Font` through one level of indirection if needed.
fn ensure_font_dict(
    inc: &mut IncrementalDocument,
    dr_id: ObjectId,
    alias: &str,
    type0_id: ObjectId,
) -> Result<(), String> {
    let font_entry = field_dict_mut(inc, dr_id)?.get(b"Font").ok().cloned();
    match font_entry {
        Some(Object::Reference(font_id)) => {
            inc.opt_clone_object_to_new_document(font_id)
                .map_err(|e| e.to_string())?;
            field_dict_mut(inc, font_id)?
                .set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
        }
        Some(Object::Dictionary(_)) => {
            let fonts = field_dict_mut(inc, dr_id)?
                .get_mut(b"Font")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            fonts.set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
        }
        _ => {
            let mut fonts = Dictionary::new();
            fonts.set(alias.as_bytes().to_vec(), Object::Reference(type0_id));
            field_dict_mut(inc, dr_id)?.set("Font", Object::Dictionary(fonts));
        }
    }
    Ok(())
}

/// Build and attach a multi-row highlight `/AP/N` on each widget.
fn draw_listbox_multi_appearances(
    inc: &mut IncrementalDocument,
    options: &[String],
    indices: &[i64],
    ap: &ApInputs,
) -> Result<(), String> {
    let encoded: Vec<Vec<u8>> = options
        .iter()
        .map(|s| appearance::encode_winansi(s))
        .collect();
    let selected: Vec<bool> = (0..options.len() as i64)
        .map(|i| indices.contains(&i))
        .collect();
    let font_ref = resolve_font_ref(inc, &ap.font_ref);
    for wb in &ap.widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let content = appearance::listbox_multi_content(
            &encoded,
            &selected,
            ap.da.size,
            w,
            h,
            &ap.da.color,
            &ap.font,
        );
        let xobj = appearance::build_appearance_xobject(content, w, h, &ap.font, font_ref);
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Build and attach a visual signature `/AP/N` on each signature widget.
fn draw_signature_appearances(
    inc: &mut IncrementalDocument,
    image: &appearance::SignatureImage,
    widgets: &[WidgetBox],
) -> Result<(), String> {
    let info = image.info();
    let image_id =
        inc.new_document
            .add_object(Object::Stream(appearance::build_signature_image_xobject(
                image.clone(),
            )));

    for wb in widgets {
        let w = wb.rect[2] - wb.rect[0];
        let h = wb.rect[3] - wb.rect[1];
        let xobj = appearance::build_signature_appearance_xobject(
            image_id,
            info.width as f32,
            info.height as f32,
            w,
            h,
        );
        let ap_id = inc.new_document.add_object(Object::Stream(xobj));

        inc.opt_clone_object_to_new_document(wb.id)
            .map_err(|e| e.to_string())?;
        let d = field_dict_mut(inc, wb.id)?;
        let mut apn = Dictionary::new();
        apn.set("N", Object::Reference(ap_id));
        d.set("AP", Object::Dictionary(apn));
    }
    Ok(())
}

/// Set /NeedAppearances false on the AcroForm, cloning whatever object holds it
/// (the Catalog if AcroForm is inline, else the AcroForm object itself).
fn clear_need_appearances(inc: &mut IncrementalDocument) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let root = prev
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let cat = prev.get_dictionary(root).map_err(|e| e.to_string())?;
    match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
            field_dict_mut(inc, id)?.set("NeedAppearances", Object::Boolean(false));
        }
        Ok(Object::Dictionary(_)) => {
            inc.opt_clone_object_to_new_document(root)
                .map_err(|e| e.to_string())?;
            let cat = field_dict_mut(inc, root)?;
            let acro = cat
                .get_mut(b"AcroForm")
                .and_then(Object::as_dict_mut)
                .map_err(|e| e.to_string())?;
            acro.set("NeedAppearances", Object::Boolean(false));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fill_fields_json, find_field};
    use lopdf::{Document, Object, ObjectId, StringFormat};

    const FICHA: &[u8] =
        include_bytes!("../../../tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
    const FICHA_OBJSTREAMS: &[u8] =
        include_bytes!("../../../tests/fixtures/generated/ficha-objstreams.pdf");
    const ANEXO: &[u8] = include_bytes!("../../../tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf");
    const FICHA_XFA: &[u8] = include_bytes!("../../../tests/fixtures/generated/ficha-xfa.pdf");
    const TINY_JPEG: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x02,
        0x00, 0x03, 0x03, 0x00, 0xff, 0xd9,
    ];

    fn reparse_value(bytes: &[u8], field_name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["value"].as_str().map(|s| s.to_string()))
    }

    fn reparse_default_value(bytes: &[u8], field_name: &str) -> Option<String> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["defaultValue"].as_str().map(|s| s.to_string()))
    }

    #[test]
    fn sets_default_value_on_text_field_without_touching_value() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","defaultValue":"DEFAULT"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        assert_eq!(
            reparse_default_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("DEFAULT")
        );
        // /V must be untouched by a /DV-only op.
        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres"),
            reparse_value(FICHA, "beneficiario.apellidos_nombres")
        );
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn sets_default_value_on_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","defaultValue":"Casado"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        assert_eq!(
            reparse_default_value(&out, "beneficiario.estado_civil").as_deref(),
            Some("Casado")
        );
    }

    #[test]
    fn sets_default_value_on_radio_as_name() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","defaultValue":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, field) = find_field(&doc, "beneficiario.tipo_beneficiario").unwrap();
        // For button fields /DV is a Name, not a string.
        assert!(matches!(field.get(b"DV").unwrap(), Object::Name(n) if n == b"Titular"));
    }

    #[test]
    fn rejects_value_and_default_value_in_same_op() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"X","defaultValue":"Y"}]"#;
        let err = fill_fields_json(FICHA, ops, &[], false).unwrap_err();
        assert!(err.contains("cannot combine defaultValue"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_default_option_for_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","defaultValue":"NotAnOption"}]"#;
        let err = fill_fields_json(FICHA, ops, &[], false).unwrap_err();
        assert!(err.contains("is not a valid option"), "got: {err}");
    }

    #[test]
    fn reset_restores_text_value_to_default() {
        let name = "beneficiario.apellidos_nombres";
        // Give the field a /DV, then a different /V, then reset.
        let with_dv = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{name}","defaultValue":"DEF"}}]"#),
            &[], false
        )
        .unwrap();
        let filled = fill_fields_json(
            &with_dv,
            &format!(r#"[{{"name":"{name}","value":"OTHER"}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&filled, name).as_deref(), Some("OTHER"));
        let reset = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{name}","reset":true}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&reset, name).as_deref(), Some("DEF"));
        Document::load_mem(&reset).unwrap();
    }

    #[test]
    fn reset_restores_value_from_indirect_default() {
        let name = "beneficiario.apellidos_nombres";
        // Point the field's /DV at an indirect string object (as some writers do).
        let mut doc = Document::load_mem(FICHA).unwrap();
        let dv_id = doc.add_object(Object::String(
            b"DEF".to_vec(),
            StringFormat::Literal,
        ));
        let (field_id, _) = find_field(&doc, name).unwrap();
        doc.get_dictionary_mut(field_id)
            .unwrap()
            .set("DV", Object::Reference(dv_id));
        let mut with_dv = Vec::new();
        doc.save_to(&mut with_dv).unwrap();

        let filled = fill_fields_json(
            &with_dv,
            &format!(r#"[{{"name":"{name}","value":"OTHER"}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&filled, name).as_deref(), Some("OTHER"));
        let reset = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{name}","reset":true}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&reset, name).as_deref(), Some("DEF"));
        Document::load_mem(&reset).unwrap();
    }

    #[test]
    fn reset_clears_text_value_when_no_default() {
        let name = "beneficiario.apellidos_nombres";
        let filled = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{name}","value":"OTHER"}}]"#),
            &[], false
        )
        .unwrap();
        let reset = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{name}","reset":true}}]"#),
            &[], false
        )
        .unwrap();
        let v = reparse_value(&reset, name);
        assert!(
            v.is_none() || v.as_deref() == Some(""),
            "expected cleared, got {v:?}"
        );
    }

    #[test]
    fn reset_restores_radio_default_as_name() {
        let name = "beneficiario.tipo_beneficiario";
        let with_dv = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{name}","defaultValue":"Titular"}}]"#),
            &[], false
        )
        .unwrap();
        let reset = fill_fields_json(
            &with_dv,
            &format!(r#"[{{"name":"{name}","reset":true}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&reset, name).as_deref(), Some("Titular"));
        let doc = Document::load_mem(&reset).unwrap();
        let (_, field) = find_field(&doc, name).unwrap();
        assert!(matches!(field.get(b"V").unwrap(), Object::Name(n) if n == b"Titular"));
    }

    #[test]
    fn rejects_reset_combined_with_value() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"X","reset":true}]"#;
        let err = fill_fields_json(FICHA, ops, &[], false).unwrap_err();
        assert!(err.contains("reset op cannot be combined"), "got: {err}");
    }

    #[test]
    fn fills_text_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA, IGNACIO"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        // Append-only: output starts with the original bytes.
        assert!(out.len() > FICHA.len());
        assert_eq!(&out[..FICHA.len()], FICHA);
        // Re-parse via the public reader.
        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("GARCIA, IGNACIO")
        );
        // And it is still a loadable PDF.
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn fills_accented_text_value_as_pdf_text_string() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"Juan P\u00e9rez"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let (_, field) = find_field(&doc, "beneficiario.apellidos_nombres").unwrap();
        let v = field.get(b"V").unwrap();

        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("Juan P\u{e9}rez")
        );
        match v {
            Object::String(bytes, StringFormat::Hexadecimal) => {
                assert_eq!(
                    bytes,
                    &vec![
                        0xfe, 0xff, 0x00, b'J', 0x00, b'u', 0x00, b'a', 0x00, b'n', 0x00, b' ',
                        0x00, b'P', 0x00, 0xe9, 0x00, b'r', 0x00, b'e', 0x00, b'z',
                    ]
                );
            }
            _ => panic!("expected UTF-16BE hexadecimal text string"),
        }
    }

    fn reparse_field(bytes: &[u8]) -> serde_json::Value {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn fills_radio_group() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        assert_eq!(
            reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(),
            Some("Titular")
        );
    }

    #[test]
    fn radio_select_accepts_opt_label() {
        const FANCY: &[u8] = include_bytes!("../../../tests/fixtures/pdf-lib/fancy_fields.pdf");
        let ops = r#"[{"name":"Historical Figures 🐺","value":"Alexander Hamilton 🇺🇸"}]"#;
        let out = fill_fields_json(FANCY, ops, &[], false).unwrap();
        let fields: serde_json::Value =
            serde_json::from_str(&crate::forms::read_fields_json(&out).unwrap()).unwrap();
        let radio = fields
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "Historical Figures 🐺")
            .unwrap();
        assert_eq!(radio["value"], "Alexander Hamilton 🇺🇸");
    }

    #[test]
    fn fills_dropdown() {
        let ops = r#"[{"name":"beneficiario.estado_civil","value":"Casado"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        assert_eq!(
            reparse_value(&out, "beneficiario.estado_civil").as_deref(),
            Some("Casado")
        );
    }

    #[test]
    fn rejects_unknown_field() {
        let ops = r#"[{"name":"does.not.exist","value":"x"}]"#;
        let err = fill_fields_json(FICHA, ops, &[], false).unwrap_err();
        assert!(err.contains("no such field"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_radio_state() {
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Nope"}]"#;
        let err = fill_fields_json(FICHA, ops, &[], false).unwrap_err();
        assert!(err.contains("on-state"), "got: {err}");
    }

    /// Read a field's /AP/N stream content as a string, if present.
    fn ap_content(doc: &Document, field_name: &str) -> Option<String> {
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        let mut stack: Vec<ObjectId> = acro
            .get(b"Fields")
            .ok()?
            .as_array()
            .ok()?
            .iter()
            .filter_map(|e| e.as_reference().ok())
            .collect();
        while let Some(id) = stack.pop() {
            let Ok(d) = doc.get_dictionary(id) else {
                continue;
            };
            if crate::forms::fully_qualified_name(doc, d) == field_name {
                let n = d
                    .get(b"AP")
                    .ok()?
                    .as_dict()
                    .ok()?
                    .get(b"N")
                    .ok()?
                    .as_reference()
                    .ok()?;
                let st = doc.get_object(n).ok()?.as_stream().ok()?;
                return Some(String::from_utf8_lossy(&st.content).into_owned());
            }
            if let Ok(kids) = d.get(b"Kids").and_then(|o| o.as_array()) {
                for k in kids {
                    if let Ok(r) = k.as_reference() {
                        stack.push(r);
                    }
                }
            }
        }
        None
    }

    fn need_appearances(doc: &Document) -> Option<bool> {
        let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let cat = doc.get_dictionary(root).ok()?;
        let acro = match cat.get(b"AcroForm").ok()? {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(d) => d,
            _ => return None,
        };
        acro.get(b"NeedAppearances")
            .ok()
            .and_then(|o| o.as_bool().ok())
    }

    /// Set the Multiline flag (Ff bit 13) on a text field and return the bytes of
    /// the modified document, so we can exercise the multiline fill path on a real
    /// fixture field even though the corpus ships only single-line text fields.
    fn with_multiline_flag(bytes: &[u8], field_name: &str) -> Vec<u8> {
        let mut doc = Document::load_mem(bytes).unwrap();
        let (id, _) = find_field(&doc, field_name).unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        d.set("Ff", Object::Integer(1 << 12));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn multiline_text_fill_wraps_into_multiple_lines() {
        // Confirm the target field is wide-but-short enough to force a wrap. The
        // value is long with spaces so greedy wrapping must break it across lines.
        let base = with_multiline_flag(FICHA, "beneficiario.apellidos_nombres");
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"the quick brown fox jumps over the lazy dog several times to overflow"}]"#;
        let out = fill_fields_json(&base, ops, &[], false).unwrap();
        Document::load_mem(&out).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
        assert!(ap.contains("TL"), "multiline AP should set leading: {ap}");
        assert!(
            ap.matches(" Tj").count() >= 2,
            "expected multiple Tj (wrapped lines), got: {ap}"
        );
    }

    #[test]
    fn text_fill_generates_appearance() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.apellidos_nombres").expect("AP/N present");
        assert!(ap.contains("(GARCIA) Tj"), "got: {ap}");
        assert!(ap.contains("Tf"));
    }

    #[test]
    fn fill_flips_need_appearances_false() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"X"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        assert_eq!(need_appearances(&doc), Some(false));
    }

    #[test]
    fn radio_fill_does_not_add_appearance_stream() {
        // Buttons already have /AP; we must not overwrite with a text stream.
        let ops = r#"[{"name":"beneficiario.tipo_beneficiario","value":"Titular"}]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        Document::load_mem(&out).unwrap(); // still valid
        assert_eq!(
            reparse_value(&out, "beneficiario.tipo_beneficiario").as_deref(),
            Some("Titular")
        );
    }

    #[test]
    fn applies_multiple_ops_in_one_save() {
        let ops = r#"[
            {"name":"beneficiario.apellidos_nombres","value":"A"},
            {"name":"beneficiario.tipo_beneficiario","value":"Familiar"}
        ]"#;
        let out = fill_fields_json(FICHA, ops, &[], false).unwrap();
        let f = reparse_field(&out);
        let by = |n: &str| {
            f.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == n)
                .cloned()
                .unwrap()
        };
        assert_eq!(by("beneficiario.apellidos_nombres")["value"], "A");
        assert_eq!(by("beneficiario.tipo_beneficiario")["value"], "Familiar");
    }

    #[test]
    fn visual_signature_generates_image_appearance() {
        let ops = r#"[{"name":"firma.titular","imageOffset":0,"imageLength":21}]"#;
        let out = fill_fields_json(ANEXO, ops, TINY_JPEG, false).unwrap();
        assert!(out.len() > ANEXO.len());
        assert_eq!(&out[..ANEXO.len()], ANEXO);

        Document::load_mem(&out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("/DCTDecode"), "missing JPEG image XObject");
        assert!(
            s.contains("/SigImg Do"),
            "missing signature form appearance draw"
        );
    }

    #[test]
    fn visual_signature_rejects_non_signature_field() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","imageOffset":0,"imageLength":21}]"#;
        let err = fill_fields_json(FICHA, ops, TINY_JPEG, false).unwrap_err();
        assert!(err.contains("cannot set image on field"), "got: {err}");
    }

    #[test]
    fn rejects_out_of_bounds_image_range() {
        let ops = r#"[{"name":"firma.titular","imageOffset":10,"imageLength":100}]"#;
        let err = fill_fields_json(ANEXO, ops, TINY_JPEG, false).unwrap_err();
        assert!(err.contains("image range"), "got: {err}");
    }

    #[test]
    fn rejects_xfa_forms_on_fill() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"x"}]"#;
        let err = fill_fields_json(FICHA_XFA, ops, &[], false).unwrap_err();
        assert!(err.contains("XFA"), "got: {err}");
    }

    #[test]
    fn reads_widths_array_from_font_dict() {
        use lopdf::{Dictionary, Document, Object};
        let mut fd = Dictionary::new();
        fd.set("FirstChar", Object::Integer(65));
        fd.set(
            "Widths",
            Object::Array(vec![Object::Integer(500), Object::Real(750.0)]),
        );
        let doc = Document::with_version("1.3");
        let w = super::widths_from_font_dict(&doc, &fd).unwrap();
        assert_eq!(w.width(b'A'), 500);
        assert_eq!(w.width(b'B'), 750);
        assert_eq!(w.width(b'C'), 556); // default for unset codes
    }

    /// Load FICHA, set the Multiselect Ff bit on `field_name` (clearing Combo bit), return new bytes.
    fn with_multiselect(bytes: &[u8], field_name: &str) -> Vec<u8> {
        use lopdf::Document;
        let mut doc = Document::load_mem(bytes).unwrap();
        let (id, _) = find_field(&doc, field_name).unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
        // Clear the Combo flag (bit 18, 1<<17) and set Multiselect (bit 22, 1<<21).
        d.set("Ff", Object::Integer((ff & !(1 << 17)) | (1 << 21)));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn multiselect_fill_sets_v_array_and_sorted_i() {
        use lopdf::Document;
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        // Provide values out of /Opt order; expect /I sorted ascending.
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
        let out = fill_fields_json(&base, ops, &[], false).unwrap();
        // Check /V is an Array of 2 entries and /I == [1, 3].
        let doc = Document::load_mem(&out).unwrap();
        let (_, field) = find_field(&doc, "beneficiario.estado_civil").unwrap();
        let v_arr = field.get(b"V").unwrap().as_array().unwrap();
        assert_eq!(v_arr.len(), 2, "/V must be an array of 2 strings");
        let i_arr = field.get(b"I").unwrap().as_array().unwrap();
        let i: Vec<i64> = i_arr.iter().map(|o| o.as_i64().unwrap()).collect();
        // "Casado" is index 1, "Viudo" is index 3 in /Opt -> sorted [1, 3].
        assert_eq!(i, vec![1, 3]);
        // /V must be sorted by index too: Casado(1) before Viudo(3).
        let v0 = lopdf::decode_text_string(&v_arr[0]).unwrap();
        let v1 = lopdf::decode_text_string(&v_arr[1]).unwrap();
        assert_eq!(v0, "Casado", "/V[0] must be Casado (index 1)");
        assert_eq!(v1, "Viudo", "/V[1] must be Viudo (index 3)");
        Document::load_mem(&out).unwrap();
    }

    #[test]
    fn rejects_multivalue_on_single_select_listbox() {
        // estado_civil WITHOUT the Multiselect flag set - first need to make it a listbox
        // by clearing the Combo bit, but without setting Multiselect.
        use lopdf::Document;
        let mut doc = Document::load_mem(FICHA).unwrap();
        let (id, _) = find_field(&doc, "beneficiario.estado_civil").unwrap();
        let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
        let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
        // Clear Combo bit only -> becomes a single-select listbox
        d.set("Ff", Object::Integer(ff & !(1 << 17)));
        let mut base = Vec::new();
        doc.save_to(&mut base).unwrap();

        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Viudo"]}]"#;
        let err = fill_fields_json(&base, ops, &[], false).unwrap_err();
        assert!(
            err.contains("does not accept multiple values"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_invalid_option_in_multivalue_fill() {
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Casado","Nope"]}]"#;
        let err = fill_fields_json(&base, ops, &[], false).unwrap_err();
        assert!(err.contains("not a valid option"), "got: {err}");
    }

    #[test]
    fn multiselect_fill_generates_highlight_appearance() {
        let base = with_multiselect(FICHA, "beneficiario.estado_civil");
        let ops = r#"[{"name":"beneficiario.estado_civil","values":["Viudo","Casado"]}]"#;
        let out = fill_fields_json(&base, ops, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let ap = ap_content(&doc, "beneficiario.estado_civil").expect("AP/N present");
        assert!(ap.contains("0.60 0.75 0.85 rg"), "no highlight: {ap}");
        assert_eq!(ap.matches(" re").count(), 2, "expected 2 highlights: {ap}");
        assert!(ap.contains("(Casado) Tj"), "missing option text: {ap}");
    }

    #[test]
    fn fills_xref_stream_pdf_incrementally() {
        let ops = r#"[{"name":"beneficiario.apellidos_nombres","value":"GARCIA"}]"#;
        let out = fill_fields_json(FICHA_OBJSTREAMS, ops, &[], false).unwrap();
        // Still append-only.
        assert_eq!(&out[..FICHA_OBJSTREAMS.len()], FICHA_OBJSTREAMS);
        // Re-parses with the new value.
        assert_eq!(
            reparse_value(&out, "beneficiario.apellidos_nombres").as_deref(),
            Some("GARCIA")
        );
        Document::load_mem(&out).unwrap();
    }

    // -- Part B: field-flag / widget-visibility mutation ---------------------

    /// Read a field's boolean attribute (`readOnly`/`required`/`exported`) from
    /// the parsed field info.
    fn reparse_flag(bytes: &[u8], field_name: &str, key: &str) -> Option<bool> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f[key].as_bool())
    }

    /// Read the first widget's boolean visibility flag from the parsed info.
    fn reparse_widget_flag(bytes: &[u8], field_name: &str, key: &str) -> Option<bool> {
        let json = crate::forms::read_fields_json(bytes).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == field_name)
            .and_then(|f| f["widgets"].as_array())
            .and_then(|w| w.first())
            .and_then(|w| w[key].as_bool())
    }

    const TEXT_FIELD: &str = "beneficiario.apellidos_nombres";

    #[test]
    fn set_read_only_flag_toggles_both_ways() {
        assert_eq!(reparse_flag(FICHA, TEXT_FIELD, "readOnly"), Some(false));
        let on = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"readOnly":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&on, TEXT_FIELD, "readOnly"), Some(true));
        // Output is still append-only.
        assert_eq!(&on[..FICHA.len()], FICHA);

        let off = fill_fields_json(
            &on,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"readOnly":false}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&off, TEXT_FIELD, "readOnly"), Some(false));
    }

    #[test]
    fn set_required_and_no_export_flags() {
        let out = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"required":true,"noExport":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&out, TEXT_FIELD, "required"), Some(true));
        // exported is the inverse of the NoExport flag.
        assert_eq!(reparse_flag(&out, TEXT_FIELD, "exported"), Some(false));
    }

    #[test]
    fn setting_a_flag_does_not_disturb_the_value() {
        let filled = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","value":"GARCIA"}}]"#),
            &[], false
        )
        .unwrap();
        let out = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"readOnly":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_value(&out, TEXT_FIELD).as_deref(), Some("GARCIA"));
        assert_eq!(reparse_flag(&out, TEXT_FIELD, "readOnly"), Some(true));
    }

    #[test]
    fn hide_sets_widget_hidden_flag_and_show_clears_it() {
        let hidden = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"hidden":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(
            reparse_widget_flag(&hidden, TEXT_FIELD, "hidden"),
            Some(true)
        );

        let shown = fill_fields_json(
            &hidden,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"hidden":false}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(
            reparse_widget_flag(&shown, TEXT_FIELD, "hidden"),
            Some(false)
        );
    }

    #[test]
    fn set_print_and_no_view_widget_flags() {
        let out = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"print":true,"noView":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_widget_flag(&out, TEXT_FIELD, "print"), Some(true));
        assert_eq!(reparse_widget_flag(&out, TEXT_FIELD, "noView"), Some(true));
    }

    #[test]
    fn set_multiline_flag_toggles_and_redraws_wrapped() {
        // Field starts as single-line.
        assert_eq!(reparse_flag(FICHA, TEXT_FIELD, "multiline"), Some(false));
        // Give it a value, then turn on multiline: the appearance must be
        // regenerated in wrapped (multiline) form, which emits a `TL` leading.
        let filled = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","value":"GARCIA"}}]"#),
            &[], false
        )
        .unwrap();
        let on = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"multiline":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&on, TEXT_FIELD, "multiline"), Some(true));
        let doc = Document::load_mem(&on).unwrap();
        let ap = ap_content(&doc, TEXT_FIELD).expect("AP/N present");
        assert!(
            ap.contains(" TL "),
            "multiline AP must emit a leading: {ap}"
        );
        // Value survives the flag toggle.
        assert_eq!(reparse_value(&on, TEXT_FIELD).as_deref(), Some("GARCIA"));

        // Toggling it back off restores the single-line appearance (no TL).
        let off = fill_fields_json(
            &on,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"multiline":false}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&off, TEXT_FIELD, "multiline"), Some(false));
        let doc = Document::load_mem(&off).unwrap();
        let ap = ap_content(&doc, TEXT_FIELD).expect("AP/N present");
        assert!(
            !ap.contains(" TL "),
            "single-line AP must not emit a leading: {ap}"
        );
    }

    #[test]
    fn set_comb_flag_writes_maxlen_and_draws_cells() {
        let on = fill_fields_json(
            FICHA,
            &format!(
                r#"[{{"name":"{TEXT_FIELD}","value":"AB","flags":{{"comb":true,"combMaxLen":5}}}}]"#
            ),
            &[], false
        );
        // A flags op cannot carry a value, so set value first, then toggle comb.
        assert!(on.is_err(), "value+flags must be rejected");

        let filled = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","value":"AB"}}]"#),
            &[], false
        )
        .unwrap();
        let on = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"comb":true,"combMaxLen":5}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&on, TEXT_FIELD, "comb"), Some(true));
        // /MaxLen was written from combMaxLen.
        let doc = Document::load_mem(&on).unwrap();
        let (_, field) = find_field(&doc, TEXT_FIELD).unwrap();
        assert_eq!(field.get(b"MaxLen").unwrap().as_i64().unwrap(), 5);
        // Comb draws one Tj per character (two cells for "AB").
        let ap = ap_content(&doc, TEXT_FIELD).expect("AP/N present");
        assert_eq!(
            ap.matches(") Tj").count(),
            2,
            "comb must place each glyph: {ap}"
        );
    }

    #[test]
    fn comb_without_maxlen_is_rejected() {
        let err = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"comb":true}}}}]"#),
            &[], false
        )
        .unwrap_err();
        assert!(err.contains("comb requires a maxLen"), "got: {err}");
    }

    #[test]
    fn set_password_flag_renders_empty_appearance() {
        let filled = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","value":"SECRET"}}]"#),
            &[], false
        )
        .unwrap();
        let on = fill_fields_json(
            &filled,
            &format!(r#"[{{"name":"{TEXT_FIELD}","flags":{{"password":true}}}}]"#),
            &[], false
        )
        .unwrap();
        assert_eq!(reparse_flag(&on, TEXT_FIELD, "password"), Some(true));
        let doc = Document::load_mem(&on).unwrap();
        let ap = ap_content(&doc, TEXT_FIELD).expect("AP/N present");
        // The value must not leak into the appearance stream.
        assert!(
            !ap.contains("SECRET"),
            "password value leaked into AP: {ap}"
        );
        assert!(!ap.contains(") Tj"), "password AP must draw nothing: {ap}");
        // The /V itself is preserved (only the rendering is suppressed).
        assert_eq!(reparse_value(&on, TEXT_FIELD).as_deref(), Some("SECRET"));
    }

    #[test]
    fn appearance_flags_rejected_on_non_text_field() {
        // estado_civil is a choice field, not text.
        let err = fill_fields_json(
            FICHA,
            r#"[{"name":"beneficiario.estado_civil","flags":{"multiline":true}}]"#,
            &[], false
        )
        .unwrap_err();
        assert!(err.contains("apply only to text fields"), "got: {err}");
    }

    #[test]
    fn flags_op_rejects_combination_with_value() {
        let err = fill_fields_json(
            FICHA,
            &format!(r#"[{{"name":"{TEXT_FIELD}","value":"X","flags":{{"readOnly":true}}}}]"#),
            &[], false
        )
        .unwrap_err();
        assert!(err.contains("cannot be combined"), "got: {err}");
    }

    // -- Part C: embedded-font fill (fontId) ---------------------------------

    const NOTO: &[u8] =
        include_bytes!("../../../tests/fixtures/fonts/NotoSans-Regular.subset.ttf");

    /// Base doc: one page + the given field(s), no embedded font at creation
    /// time (that's what the fill op adds).
    fn base_with_field(fields: &str) -> Vec<u8> {
        crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            &[],
            "[]",
            fields,
            false,
            false,
        )
        .unwrap()
    }

    fn fill_plan(op_json: &str, font_len: usize) -> String {
        format!(
            r#"{{"fill":[{op_json}],"draw":{{"ops":[],"fonts":[{{"offset":0,"length":{font_len},"subset":true}}]}}}}"#
        )
    }

    #[test]
    fn fills_standard14_field_with_embedded_font() {
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let plan = fill_plan(r#"{"name":"n","value":"Añb","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // /V round-trips via the public reader (UTF-16BE under the hood).
        let v = crate::forms::read_fields_json(&out).unwrap();
        assert!(v.contains(r#""value":"Añb""#), "round-trip via read_fields: {v}");
        // DA references BPF0 and /DR has it as Type0.
        let field = doc
            .objects
            .values()
            .find_map(|o| {
                let d = o.as_dict().ok()?;
                (d.get(b"T").ok()?.as_str().ok()? == b"n").then_some(d)
            })
            .unwrap();
        let da = field.get(b"DA").unwrap().as_str().unwrap();
        assert!(da.starts_with(b"/BPF0 "), "DA: {}", String::from_utf8_lossy(da));
    }

    #[test]
    fn embedded_fill_multiline_wraps() {
        let base = base_with_field(
            r#"[{"type":"text","name":"m","page":0,"x":10,"y":10,"width":60,"height":60,"multiline":true}]"#,
        );
        let plan = fill_plan(
            r#"{"name":"m","value":"aaaa bbbb cccc dddd","fontId":0}"#,
            NOTO.len(),
        );
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // Look up field "m"'s own /AP/N (HashMap object iteration order isn't
        // stable, so scanning all Form XObjects could hit an unrelated one).
        let ap = ap_content(&doc, "m").expect("AP/N present");
        let tj_count = ap.as_bytes().windows(2).filter(|w| w == b"Tj").count();
        assert!(
            tj_count >= 2,
            "expected wrapped lines, got content: {ap}"
        );
    }

    #[test]
    fn embedded_fill_missing_glyph_errors_before_write() {
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let plan = fill_plan(r#"{"name":"n","value":"日本語","fontId":0}"#, NOTO.len()); // Latin subset font
        let err = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap_err();
        assert!(err.starts_with("missing glyphs"), "got: {err}");
        assert!(err.contains("field 'n'"), "got: {err}");
    }

    #[test]
    fn embedded_fill_rejects_comb_and_choice() {
        let base = base_with_field(
            r#"[{"type":"text","name":"c","page":0,"x":10,"y":10,"width":200,"height":20,"comb":true,"maxLength":4}]"#,
        );
        let plan = fill_plan(r#"{"name":"c","value":"ab","fontId":0}"#, NOTO.len());
        let err = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap_err();
        assert!(err.contains("plain and multiline text fields only"), "got: {err}");
    }

    #[test]
    fn embedded_default_value_missing_glyph_errors_before_write() {
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let plan = fill_plan(
            r#"{"name":"n","defaultValue":"日本語","fontId":0}"#,
            NOTO.len(),
        ); // Latin subset font
        let err = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap_err();
        assert!(err.starts_with("missing glyphs"), "got: {err}");
        assert!(err.contains("field 'n'"), "got: {err}");
    }

    #[test]
    fn embedded_default_value_wires_da_and_dr_and_round_trips() {
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let plan = fill_plan(r#"{"name":"n","defaultValue":"Añb","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let field = doc
            .objects
            .values()
            .find_map(|o| {
                let d = o.as_dict().ok()?;
                (d.get(b"T").ok()?.as_str().ok()? == b"n").then_some(d)
            })
            .unwrap();
        let da = field.get(b"DA").unwrap().as_str().unwrap();
        assert!(da.starts_with(b"/BPF0 "), "DA: {}", String::from_utf8_lossy(da));
        let v = crate::forms::read_fields_json(&out).unwrap();
        assert!(
            v.contains(r#""defaultValue":"Añb""#),
            "round-trip via read_fields: {v}"
        );
        let acro = crate::forms::acroform(&doc).unwrap();
        let dr_fonts = acro
            .get(b"DR")
            .and_then(|o| o.as_dict())
            .and_then(|dr| dr.get(b"Font"))
            .and_then(|o| o.as_dict())
            .unwrap();
        assert!(dr_fonts.has(b"BPF0"), "DR/Font must have BPF0: {dr_fonts:?}");
    }

    #[test]
    fn embedded_value_and_default_value_same_font_merge_dr_and_round_trip() {
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let plan = format!(
            r#"{{"fill":[{{"name":"n","value":"Añb","fontId":0}},{{"name":"n","defaultValue":"Bñc","fontId":0}}],"draw":{{"ops":[],"fonts":[{{"offset":0,"length":{},"subset":true}}]}}}}"#,
            NOTO.len()
        );
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        let acro = crate::forms::acroform(&doc).unwrap();
        let dr_fonts = acro
            .get(b"DR")
            .and_then(|o| o.as_dict())
            .and_then(|dr| dr.get(b"Font"))
            .and_then(|o| o.as_dict())
            .unwrap();
        let bpf0_count = dr_fonts.iter().filter(|(k, _)| k.as_slice() == b"BPF0").count();
        assert_eq!(bpf0_count, 1, "expected a single BPF0 entry: {dr_fonts:?}");
        let v = crate::forms::read_fields_json(&out).unwrap();
        assert!(v.contains(r#""value":"Añb""#), "value round-trip: {v}");
        assert!(
            v.contains(r#""defaultValue":"Bñc""#),
            "defaultValue round-trip: {v}"
        );
    }

    #[test]
    fn refilling_builder_embedded_field_now_works() {
        // The fixture from the old rejects_filling_a_type0_da_font_field test -
        // now with fontId it succeeds.
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, NOTO.len());
        let fields = r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20,"value":"A","fontId":0}]"#;
        let base = crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            NOTO,
            &fonts_json,
            fields,
            false,
            false,
        )
        .unwrap();
        let plan = fill_plan(r#"{"name":"n","value":"B","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        let v = crate::forms::read_fields_json(&out).unwrap();
        assert!(v.contains(r#""value":"B""#), "{v}");
    }

    /// Rewrite `base`'s AcroForm so its `/DR` is an indirect reference to a
    /// separate object (mirroring Acrobat-authored PDFs), instead of the
    /// inline dict the builder normally produces. Returns the re-saved bytes.
    fn make_dr_indirect(base: &[u8]) -> Vec<u8> {
        use lopdf::Document;
        let mut doc = Document::load_mem(base).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root).unwrap();
        let acro_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acro = doc.get_dictionary(acro_id).unwrap();
        let dr = acro.get(b"DR").unwrap().as_dict().unwrap().clone();
        let dr_id = doc.add_object(Object::Dictionary(dr));
        let acro_mut = doc.get_object_mut(acro_id).unwrap().as_dict_mut().unwrap();
        acro_mut.set("DR", Object::Reference(dr_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Rewrite `base`'s AcroForm so `/DR` stays inline but `/DR/Font` becomes
    /// an indirect reference to its own object. Returns the re-saved bytes.
    fn make_dr_font_indirect(base: &[u8]) -> Vec<u8> {
        use lopdf::Document;
        let mut doc = Document::load_mem(base).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root).unwrap();
        let acro_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acro = doc.get_dictionary(acro_id).unwrap();
        let fonts = acro
            .get(b"DR")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap()
            .clone();
        let font_id = doc.add_object(Object::Dictionary(fonts));
        let acro_mut = doc.get_object_mut(acro_id).unwrap().as_dict_mut().unwrap();
        acro_mut
            .get_mut(b"DR")
            .and_then(Object::as_dict_mut)
            .unwrap()
            .set("Font", Object::Reference(font_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Resolve `/Root/AcroForm/DR/Font` through any references in `out` and
    /// assert both the pre-existing `Helv` and the new `BPF0` are present.
    fn assert_dr_fonts_has_helv_and_bpf0(out: &[u8]) {
        let doc = Document::load_mem(out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root).unwrap();
        let acro = crate::forms::as_dict(&doc, catalog.get(b"AcroForm").unwrap()).unwrap();
        let dr = crate::forms::as_dict(&doc, acro.get(b"DR").unwrap()).unwrap();
        let fonts = crate::forms::as_dict(&doc, dr.get(b"Font").unwrap()).unwrap();
        assert!(
            fonts.has(b"Helv"),
            "existing /DR/Font/Helv must survive the fill: {fonts:?}"
        );
        assert!(
            fonts.has(b"BPF0"),
            "new embedded font BPF0 must be added to /DR/Font: {fonts:?}"
        );
    }

    #[test]
    fn embedded_fill_preserves_inline_dr_with_indirect_font() {
        // /DR inline dict, but /DR/Font is an indirect reference.
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let base = make_dr_font_indirect(&base);
        let plan = fill_plan(r#"{"name":"n","value":"Añb","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        assert_dr_fonts_has_helv_and_bpf0(&out);
    }

    #[test]
    fn embedded_fill_preserves_indirect_dr_with_indirect_font() {
        // Both /DR and /DR/Font are indirect references.
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let base = make_dr_indirect(&make_dr_font_indirect(&base));
        let plan = fill_plan(r#"{"name":"n","value":"Añb","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();
        assert_dr_fonts_has_helv_and_bpf0(&out);
    }

    #[test]
    fn embedded_fill_preserves_indirect_dr_font_entries() {
        // AcroForm authored with an indirect /DR (as Acrobat typically does),
        // instead of the builder's inline /DR dict.
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let base = make_dr_indirect(&base);
        let plan = fill_plan(r#"{"name":"n","value":"Añb","fontId":0}"#, NOTO.len());
        let out = crate::apply::apply_all_json(&base, &plan, &[], &[], NOTO, &[], false).unwrap();

        let doc = Document::load_mem(&out).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root).unwrap();
        let acro = crate::forms::as_dict(&doc, catalog.get(b"AcroForm").unwrap()).unwrap();
        let dr = crate::forms::as_dict(&doc, acro.get(b"DR").unwrap()).unwrap();
        let fonts = crate::forms::as_dict(&doc, dr.get(b"Font").unwrap()).unwrap();
        assert!(
            fonts.has(b"Helv"),
            "existing /DR/Font/Helv must survive fill through an indirect /DR: {fonts:?}"
        );
        assert!(
            fonts.has(b"BPF0"),
            "new embedded font BPF0 must be added to /DR/Font: {fonts:?}"
        );
    }

    #[test]
    fn type0_da_fill_without_font_gives_actionable_error() {
        // Same base as above, but fill WITHOUT fontId.
        let fonts_json = format!(r#"[{{"offset":0,"length":{},"subset":true}}]"#, NOTO.len());
        let fields = r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20,"value":"A","fontId":0}]"#;
        let base = crate::create::create_document_json(
            r#"[{"op":"addPage","width":300,"height":300}]"#,
            &[],
            NOTO,
            &fonts_json,
            fields,
            false,
            false,
        )
        .unwrap();
        let err = fill_fields_json(&base, r#"[{"name":"n","value":"B"}]"#, &[], false).unwrap_err();
        assert!(err.contains("pass { font }"), "got: {err}");
    }

    #[test]
    fn fills_orphaned_widget_field_by_name() {
        // An orphaned widget field (on the page, absent from /AcroForm/Fields)
        // is fillable via find_field's page-annots fallback and round-trips.
        const ISS: &[u8] =
            include_bytes!("../../../tests/fixtures/pypdf/issues/iss2453-ExampleForm.pdf");
        let out = fill_fields_json(ISS, r#"[{"name":"Contact Name","value":"Ada"}]"#, &[], false)
            .unwrap();
        assert_eq!(reparse_value(&out, "Contact Name").as_deref(), Some("Ada"));
    }

    #[test]
    fn da_font_base_maps_names_and_aliases() {
        assert_eq!(super::da_font_base("Helvetica"), Some("Helvetica"));
        assert_eq!(super::da_font_base("Helv"), Some("Helvetica"));
        assert_eq!(super::da_font_base("TiRo"), Some("Times-Roman"));
        assert_eq!(super::da_font_base("Courier-Bold"), Some("Courier-Bold"));
        assert_eq!(super::da_font_base("CoBO"), Some("Courier-BoldOblique"));
        // Symbol/ZapfDingbats and unknown fonts are not synthesized.
        assert_eq!(super::da_font_base("ZaDb"), None);
        assert_eq!(super::da_font_base("Arial"), None);
    }

    /// Drop `/DR` from the AcroForm so the field's DA font ("Helv") no longer
    /// resolves to a `/DR/Font` object, exercising the standard-14 synth path.
    fn strip_dr(base: &[u8]) -> Vec<u8> {
        let mut doc = Document::load_mem(base).unwrap();
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root).unwrap();
        let acro_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acro_mut = doc.get_object_mut(acro_id).unwrap().as_dict_mut().unwrap();
        acro_mut.remove(b"DR");
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Resolve the field's `/AP/N` stream and return its
    /// `/Resources/Font/<name>` dictionary (dereferenced).
    fn ap_resource_font(doc: &Document, field_name: &str) -> Option<lopdf::Dictionary> {
        let (_, field) = find_field(doc, field_name)?;
        let ap_id = field
            .get(b"AP")
            .ok()?
            .as_dict()
            .ok()?
            .get(b"N")
            .ok()?
            .as_reference()
            .ok()?;
        let st = doc.get_object(ap_id).ok()?.as_stream().ok()?;
        let fonts = crate::forms::as_dict(doc, st.dict.get(b"Resources").ok()?)
            .ok()?
            .get(b"Font")
            .ok()?;
        let fonts = crate::forms::as_dict(doc, fonts).ok()?;
        let (_, entry) = fonts.iter().next()?;
        crate::forms::as_dict(doc, entry).ok().cloned()
    }

    #[test]
    fn fills_std14_da_font_absent_from_dr_by_synthesizing() {
        // A text field whose DA names /Helv, but with /DR removed: the DA font
        // is a standard-14 font not present in /DR (mirrors IRS f1040, #2670).
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        let base = strip_dr(&base);
        // Fill succeeds instead of erroring "DA font 'Helv' not found in /DR".
        let out = fill_fields_json(&base, r#"[{"name":"n","value":"Brooks"}]"#, &[], false).unwrap();
        // Append-only save preserved.
        assert_eq!(&out[..base.len()], &base[..]);
        assert_eq!(reparse_value(&out, "n").as_deref(), Some("Brooks"));

        let doc = Document::load_mem(&out).unwrap();
        // The appearance draws the value...
        let content = ap_content(&doc, "n").expect("AP/N present");
        assert!(content.contains("(Brooks) Tj"), "value not drawn: {content}");
        // ...and its /Resources/Font carries a synthesized Type1 Helvetica.
        let fd = ap_resource_font(&doc, "n").expect("AP font resource present");
        assert_eq!(
            fd.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Type1",
            "synthesized font must be Type1: {fd:?}"
        );
        assert_eq!(
            fd.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Helvetica",
            "synthesized /BaseFont must be Helvetica: {fd:?}"
        );
    }

    #[test]
    fn unknown_da_font_absent_from_dr_still_errors() {
        // A non-standard DA font that is missing from /DR cannot be synthesized
        // and must still surface the actionable "not found in /DR" error.
        let base = base_with_field(
            r#"[{"type":"text","name":"n","page":0,"x":10,"y":10,"width":200,"height":20}]"#,
        );
        // Point the field's /DA at a font that is neither in /DR nor standard-14.
        let mut doc = Document::load_mem(&base).unwrap();
        let (id, _) = find_field(&doc, "n").unwrap();
        doc.get_object_mut(id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("DA", Object::string_literal("/Wingding 0 Tf 0 g"));
        let mut base = Vec::new();
        doc.save_to(&mut base).unwrap();
        let base = strip_dr(&base);

        let err = fill_fields_json(&base, r#"[{"name":"n","value":"x"}]"#, &[], false).unwrap_err();
        assert!(err.contains("not found in /DR"), "got: {err}");
    }
}
