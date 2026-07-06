---
title: "AcroForms: how fields really work"
description: A form field is two things at once — a data node in a field tree and a widget annotation drawn on a page. Understanding the split explains filling, appearances, and flattening.
---

"The form" isn't a layer sitting on top of a PDF, separate from the page
content the previous article walked through. It's more objects in the same
graph: a `/Fields` array hanging off the Catalog, dictionaries that hold
values, and annotations that place those values on a page. Filling a form
means editing an object graph; rendering it means running the content
streams those objects point to. Everything below follows from that.

## A field on disk

```text title="A text field: field dictionary and widget annotation merged"
1 0 obj                        % the Catalog grows an /AcroForm entry
<< /Type /Catalog /Pages 2 0 R
   /AcroForm << /Fields [6 0 R] /DA (/Helv 0 Tf 0 g) >> >>
endobj

6 0 obj
<< /FT /Tx                     % field type: text
   /T (applicant.name)         % field name
   /V (Ada Lovelace)           % current value — the DATA
   /Type /Annot                % …and, merged in, a widget annotation:
   /Subtype /Widget
   /Rect [72 680 300 704]      % where it sits on the page — the PIXELS
   /AP << /N 7 0 R >>          % appearance stream: how the value LOOKS
   /DA (/Helv 12 Tf 0 g) >>
endobj

7 0 obj                        % the appearance: a small content stream
<< /Type /XObject /Subtype /Form /BBox [0 0 228 24] /Length 41 >>
stream
BT /Helv 12 Tf 2 6 Td (Ada Lovelace) Tj ET
endstream
endobj
```

Object 6 is doing two jobs in one dictionary. `/FT`, `/T`, and `/V` describe
a piece of data — a text field named `applicant.name` currently holding the
string "Ada Lovelace." `/Type /Annot`, `/Subtype /Widget`, and `/Rect`
describe a rectangle on a page, the same annotation mechanism links, popups,
and other on-page markup use. The PDF spec lets these collapse into a single
object when a field has exactly one widget annotation, which is why the
dump above only needs one dictionary rather than two linked by a reference.

## Data vs pixels

`/V` is the value; `/AP` is what a viewer actually paints, and the two are
tracked separately on purpose. `/AP` points at a Form XObject — the same
kind of small, self-contained content stream the previous article covered —
whose `Tj` operator paints the literal glyphs "Ada Lovelace" inside a
228×24 box. A viewer paints the page's `/Contents` first, then walks the
page's `/Annots` array in a second pass and stamps each widget's `/AP /N`
appearance at its `/Rect` — at no point in producing pixels does it read
`/V`.

That split is also the source of a familiar bug. If something sets `/V` to
a new string but leaves the old `/AP` stream untouched, the field now holds
the right data and shows the wrong pixels — the classic "I typed a value but
it doesn't show until I click the field" symptom, because clicking is what
makes an interactive viewer regenerate the appearance on the fly. Anything
that fills a field programmatically, without a human clicking through it,
has to regenerate `/AP` itself at fill time, or the output will look empty
or stale to any renderer that doesn't bother synthesizing appearances on
open.

## The field tree and names

Fields don't have to be flat. A field dictionary can carry `/Kids`,
pointing at child field dictionaries (or at widget annotations, for fields
with several widgets), and a child inherits properties like `/FT` and
`/DA` from its parent unless it overrides them. The field's full name is
built by walking up that chain and joining every ancestor's `/T` value with
a dot — which means `applicant.name` in the dump above could be exactly
what it looks like, one field literally named `applicant.name`, or it could
be a field named `name` nested under a parent field named `applicant`,
producing the identical fully-qualified name through concatenation. The two
are byte-different on disk — one dictionary versus a parent/child pair —
but name-identical to anything that only looks at the joined string.

## Flattening, precisely

Flattening a field is the inverse of the data/pixels split: instead of
keeping `/V` and `/AP` as two live, editable things, it takes the current
appearance and bakes it permanently into the page. Concretely, the `/AP`
stream's operators are copied into the page's own content stream (with the
widget's position and matrix accounted for), and the widget annotation and
its field dictionary are removed from `/Annots` and `/Fields`. The visible
text — the pixels — stays exactly where it was. What's gone is the
interactivity: there's no more `/V` to read, no more field to click into,
no more entry in `/AcroForm` pointing at it. A flattened field is just page
content now, indistinguishable from text a document was drawn with from the
start.

## In better-pdf

Field names are surfaced fully qualified — "ancestor `/T` joined by
`.`" — and on creation "each name is the field's literal, fully-qualified
name; it is not split into a parent/child hierarchy on dots"
([Creating form fields](/better-pdf/guides/creating-form-fields/)).

`FieldInfo.widgets` has "one entry per widget annotation (page + position).
Usually one; radio groups and fields repeated across pages have several"
([API](/better-pdf/reference/api/)).

The typed-forms generator (`better-pdf-generate-types form.pdf
src/form-types.ts`) turns this name model into compile-time safety: unknown
names and invalid options become compile errors
([Typed forms](/better-pdf/guides/typed-forms/)).

Unknown names at runtime throw `UnknownFieldError`; invalid choice values
throw `InvalidOptionError` ([Errors](/better-pdf/reference/errors/)).

Next: [Fonts](/better-pdf/internals/fonts/).
