---
title: Fonts & text encoding
description: PDF strings are not text — they are glyph codes whose meaning depends on the font's encoding. From the standard 14 to Type0 composites, here is how PDFs turn bytes into visible words.
---

A content stream operator like `(Hello) Tj` only *happens* to look like
readable text. The bytes inside those parentheses are not Unicode — they're
codes, and what they mean depends entirely on the font dictionary the current
`Tf` operator points at. Change the font and the same five bytes could paint
different glyphs, or none at all. There's no global alphabet a PDF reader
falls back on; every string is only as meaningful as the font resource
sitting behind it. Reading a PDF's text is really reading its fonts.

## The simplest case: a standard font

The smallest possible font object is a name and nothing else:

```text title="A Type1 standard font — no font file embedded"
4 0 obj
<< /Type /Font /Subtype /Type1
   /BaseFont /Helvetica >>     % viewer supplies the font — one of the "standard 14"
endobj
```

There's no font *program* here — no outlines, no glyph data, nothing to
embed. The dictionary just names a font and trusts the viewer to already have
it.

## The standard 14 and WinAnsi

That trust isn't blind faith — it's a spec guarantee. The PDF spec requires
every conforming viewer to ship 14 built-in fonts: the Helvetica, Times, and
Courier families (four styles each) plus Symbol and ZapfDingbats. Reference
one by name, as object 4 does, and any viewer anywhere can render it without
you shipping a single byte of font data.

The catch is encoding. Standard-14 text uses single-byte character codes,
typically under WinAnsi encoding — one byte, one glyph, 256 possible codes
per font. That's plenty for English and most Western European languages,
where accented letters like é or ñ each land on their own code point within
that single byte. It has no way to represent Chinese, Japanese, Korean, or
really any script with more than a couple hundred characters, though. A
single byte can't index a glyph set that size, no matter how the encoding
table is arranged — you'd run out of codes long before you ran out of
glyphs.

## Composite (Type0) fonts

The fix is composite fonts, and they change the encoding model at its root:

```text title="A Type0 composite font — embedded, multi-byte, Unicode-mappable"
8 0 obj
<< /Type /Font /Subtype /Type0
   /BaseFont /NotoSansJP
   /Encoding /Identity-H       % 2-byte codes map straight to glyph IDs
   /DescendantFonts [9 0 R]    % → CIDFontType2 wrapping an embedded TrueType
   /ToUnicode 10 0 R >>        % CMap: glyph IDs back to Unicode (copy/paste, search)
endobj
```

Instead of one byte per character, a Type0 font under `/Identity-H` reads
two-byte codes — enough distinct values to cover a font with thousands of
glyphs. Object 8 isn't the whole story either: `/DescendantFonts` points at a
CIDFontType2 dictionary that carries the actual embedded TrueType font
program, the outlines a rasterizer needs to draw each glyph. Object 8 is the
addressing scheme; object 9 is the ink.

The last entry matters more than its one line suggests. `/ToUnicode` is a
CMap stream that maps each glyph code back to a Unicode code point — it's the
only reason copy-pasting or searching text in a PDF viewer produces the
characters you'd expect. Glyph codes and Unicode code points are unrelated
namespaces; a font can embed and render glyphs perfectly while leaving a
viewer with no idea what character each glyph corresponds to. Without
`/ToUnicode`, text extraction is guesswork built on font-specific tables and
heuristics, not a lookup.

## Why text extraction is hard

Put those two sections together and it's clear why "extract the text" is
never as simple as it sounds. Encoding isn't fixed across a document — it
varies font by font, sometimes page by page, so a byte that means one glyph
under one font means something else entirely under the next. A tool that
walks a page's content stream has to track which font each `Tf` selected
before it can even guess what a following string means, and the same raw
bytes decode to different characters on the next page if that page picked a
different font.

Subsetting, the common practice of embedding only the glyphs a document
actually uses, typically renumbers glyph IDs in the process, so the same
source font produces different codes in different PDFs — there's no fixed
mapping from "glyph 42" to a particular letter across documents, only within
one. And plenty of real-world producers simply omit `/ToUnicode`, especially
for standard-14 text where authors assume WinAnsi is "close enough" to ASCII
to not bother. Any tool that extracts text is really reconstructing intent
from whatever encoding information the producer happened to leave behind,
not performing a lookup against some universal table.

## In better-pdf

`StandardFonts` exposes 12 of the standard 14 — "Symbol and ZapfDingbats are
intentionally omitted"; text in standard fonts "is limited to the WinAnsi
charset" ([StandardFonts](/better-pdf/api-reference/enumerations/standardfonts/),
[API](/better-pdf/reference/api/)).

`embedFont()` embeds any TTF/OTF "to render Unicode text — including CJK
characters" as "a PDF Type0/CIDFontType2 composite with a ToUnicode CMap, so
text is selectable and searchable"
([Generating & drawing](/better-pdf/guides/generating/)).

Current limitation: re-filling an embedded-font form field through the form
API is not yet supported and throws; form-field values render with
standard-14 fonts ([Limitations](/better-pdf/reference/limitations/)).

Next: [Compression](/better-pdf/internals/compression/).
