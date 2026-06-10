# Milestone 15 — Field Layout (required + widget page/rect)

**Status:** ✅ Implemented and merged.

**Goal:** Surface, per field, whether it is required and where each of its widget
annotations sits (which page, and its rectangle), so callers can drive overlays,
validation, and layout-aware UIs.

## What shipped

- **`FieldInfo.required`** — `Ff & 2` (the Required flag).
- **`FieldInfo.widgets: FieldWidget[]`** — one entry per widget annotation, each
  with a 0-based `page` index and `rect` `[x0, y0, x1, y1]` in PDF points
  (origin bottom-left). Most fields have one; radio groups and fields repeated
  across pages have several.

## Implementation notes

- De-risked by confirming `flatten.rs` already had the widget → page → rect
  machinery. Made it reusable: `pub(crate) struct RawWidget { id, page_id, rect }`
  and `pub(crate) fn field_widgets(...)`.
- `forms.rs` builds an `ObjectId → page index` map from `doc.get_pages()` and maps
  each widget's `/P` to a page index; `describe_field` filters/maps `field_widgets`
  into `Widget { page, rect }`.
- The type generator also emits `required`.

## Files

- Modify `crates/core/src/forms.rs` (FieldInfo + Widget + page map + Rust test),
  `crates/core/src/flatten.rs` (reusable widget helpers), `src/form.ts`
  (`FieldWidget` + new `FieldInfo` fields), `src/typegen.ts`, `README.md`,
  `skills/better-pdf/SKILL.md`. Add `tests/field-layout.test.ts`.

## Decision (confirmed with user)

- Position is exposed as a **widgets array** (page + rect per annotation), rather
  than a single page/rect, because one field can have multiple widgets.
