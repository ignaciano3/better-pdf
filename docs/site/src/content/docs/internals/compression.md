---
title: Compression & object streams
description: PDFs shrink in two ways — filters that deflate individual streams, and object streams that pack whole objects together. Here is what /FlateDecode and /ObjStm actually do.
---

The `hello.pdf` used throughout this section was chosen because it's all
plain text — every byte, opened in an editor, reads as ASCII you can
recognize. Real PDFs almost never look like that. The moment a file has more
than a token amount of text, images, or embedded fonts, most of its bytes
stop being readable, and a hex dump shows structure and noise interleaved.
PDF gets there through two separate mechanisms, aimed at two different
targets: **filters**, which compress the data inside one stream, and
**object streams**, which compress the *structure* — the dictionaries that
describe the document — by packing many of them into a single stream. They
solve different problems and neither replaces the other.

## Filters: compressing stream data

```text title="The same content stream, deflated"
5 0 obj
<< /Length 38                  % length of the COMPRESSED bytes
   /Filter /FlateDecode >>     % decompress with zlib/deflate before use
stream
x\x9c\x0bA\x11… (38 bytes of deflate data)
endstream
endobj
```

This is object 5 again — the same slot that held the content stream in the
anatomy article's `hello.pdf`, playing the same role: a stream of drawing
operators, the language the content-streams article showed in the clear.
On the anatomy page it appeared uncompressed, `/Length 44`, readable top to
bottom as `BT /F1 24 Tf … Tj ET`. Here it's the same kind of stream,
run through `/FlateDecode` first: the dictionary now carries a `/Filter`
entry naming the compression scheme, `/Length` describes the *compressed*
byte count, and the bytes between `stream` and `endstream` are opaque
deflate output rather than an operator you could read by eye. A reader sees
`/Filter /FlateDecode`, inflates the bytes before handing them to whatever
consumes the stream, and everything downstream — the operator list, the
font's glyph program, an image's raw pixels — is unaffected by the fact that
it arrived compressed.

Filters apply per stream, independently, and the win varies enormously with
what's inside. Content streams and font programs are mostly repetitive text
and structure, so they deflate hard — cutting a stream to a fraction of its
original size is routine. Streams that are already compressed by a
format-specific scheme — JPEG image data, most embedded font programs — gain
little to nothing from a second pass of deflate on top, since there's no
redundancy left for it to find; a writer that recompresses them anyway is
just spending CPU time to produce the same number of bytes, or occasionally
more.

## Object streams: compressing the structure

```text title="An object stream: many small objects inside one compressed stream"
12 0 obj
<< /Type /ObjStm
   /N 3                        % holds 3 objects
   /First 18                   % byte where the first object's data starts
   /Filter /FlateDecode /Length 74 >>
stream
1 0 4 52 6 61                  % pairs: object number, offset inside the stream
<< /Type /Catalog … >>         % object 1
<< /Type /Font … >>            % object 4
<< /FT /Tx /T (name) … >>      % object 6
endstream
endobj
```

Filters shrink the handful of large streams a document has — content,
fonts, images. But a document with a big form or hundreds of pages can also
have thousands of *small* objects: individual field dictionaries, page
dictionaries, font resource entries — each just a few dozen bytes, each
paying the fixed overhead of its own `N 0 obj … endobj` wrapper and its own
20-byte xref entry. An object stream, `/Type /ObjStm`, is a container: `/N`
objects' worth of dictionaries, laid end to end and deflated together as one
stream, indexed by a small header (`/First` plus the object-number/offset
pairs) so that once the stream is inflated, a reader can jump straight to
object 4 or object 6 by offset instead of re-parsing the dictionaries before
it. Packing many small dictionaries into one
compressed blob compresses the redundancy *between* them — repeated key
names, similar structure — not just the bytes inside any single one. Object
streams arrived in PDF 1.5 alongside a companion feature: since an object
inside one no longer has an ordinary file offset to record, the xref table
itself can be stored the same way, as a compressed **cross-reference
stream**, rather than the plain-text table shown in the anatomy article.

## The trade-off

Both an `/ObjStm` and the cross-reference stream that indexes it are
computed from the whole set of objects that end up inside them — adding,
removing, or resizing even one object changes what the container holds and
where things sit inside it. That means object streams are a property of a
*full-document* write, one that lays out every object from scratch, not a
patch applied to an existing file. They're fundamentally incompatible with
the append-only editing model described in the incremental-updates article:
an incremental save works by leaving every original byte untouched and
tacking a small addition on the end, and there's no way to slot a new object
into an already-deflated, already-indexed `/ObjStm` without rewriting it —
at which point it isn't an incremental update anymore.

## In better-pdf

`compress` (on by default) deflates generated content/appearance/font
streams and leaves already-compressed streams untouched; on incremental
saves "only the newly appended section is compressed; the original
revision's bytes are preserved" ([Generating & drawing](/better-pdf/guides/generating/),
[API](/better-pdf/reference/api/)).

`objectStreams` (off by default) "applies only to full-document saves —
`create()`, `merge`, `assemble`, `copyPages`, `splitPages`. Incremental
(loaded-document) saves ignore it and remain append-only, so existing
signatures stay valid" ([Generating & drawing](/better-pdf/guides/generating/)).

The output-size effect is measured on the
[Benchmarks](/better-pdf/reference/benchmarks/) page.

That's the last piece of the tour — back to
[How PDFs work](/better-pdf/internals/) for the full map.
