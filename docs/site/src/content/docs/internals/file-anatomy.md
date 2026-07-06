---
title: Anatomy of a PDF file
description: Every PDF is four parts — header, body, cross-reference table, and trailer. Here is a complete, working 6-object PDF, annotated line by line.
---

A PDF is not a black box. Open one in a text editor and, underneath the
compressed images and font data, most of what makes it a PDF is plain,
readable text: a version tag, a list of numbered objects, a lookup table, and
a short trailer telling a reader where to start. This page walks through a
complete, minimal, valid PDF — six objects, one page, one line of text — and
names every part of it. Later articles in this section go deeper on each
piece; this one is the map.

## The whole file

```text title="hello.pdf — a complete PDF (offsets illustrative)"
%PDF-1.7                       % 1. HEADER: version of the spec this file uses
%âãÏÓ                          %    four bytes > 127 so tools treat the file as binary

1 0 obj                        % 2. BODY: numbered objects. "1 0" = object 1, generation 0
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842]
   /Resources << /Font << /F1 4 0 R >> >>
   /Contents 5 0 R >>
endobj
4 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
5 0 obj
<< /Length 44 >>
stream
BT /F1 24 Tf 72 770 Td (Hello, world!) Tj ET
endstream
endobj

xref                           % 3. CROSS-REFERENCE TABLE: byte offset of every object
0 6                            %    entries for objects 0–5
0000000000 65535 f             %    object 0 is always free
0000000017 00000 n             %    object 1 starts at byte 17
0000000066 00000 n
0000000119 00000 n
0000000251 00000 n
0000000320 00000 n

trailer                        % 4. TRAILER: where to start reading
<< /Size 6 /Root 1 0 R >>      %    /Root points at the Catalog — object 1
startxref
408                            %    byte offset of the "xref" line above
%%EOF
```

## Header

The first line, `%PDF-1.7`, is the header: a comment (PDF comments start with
`%`) declaring which version of the PDF specification the file follows. It's
a hint about which features the file may use — later versions add
capabilities (object streams, newer encryption, and so on) — though in
practice readers parse whatever they find rather than gating features on
this line. `1.7` is by far the most common version in the wild.

The second line is stranger: `%âãÏÓ`, four bytes whose values are all above
127. This isn't a version number or metadata — it exists purely so that
software which peeks at the first few hundred bytes of a file to guess
whether it's "binary" or "text" gets the right answer. Some file-transfer
tools historically mangled line endings or stripped high-bit bytes from
files they mistook for plain text, which would silently corrupt a PDF. The
comment's high-bit bytes flag the file as binary before any object content
appears, so those tools leave it alone. The exact bytes don't matter — only
that at least four of them are outside the 7-bit ASCII range — but `%âãÏÓ`
is the sequence recommended by the PDF specification and used almost
universally.

Together these two lines are the entire header: two comment lines, both
optional in the strictest technical sense, both present in essentially every
real-world PDF.

## Body

Everything between the header and the cross-reference table is the body: a
flat sequence of **objects**. Each object is introduced by two integers and
the keyword `obj` — for example `1 0 obj` — and closed by `endobj`. The first
integer is the **object number**, a stable identifier used to refer to this
object from elsewhere in the file; the second is the **generation number**,
which increments if the object is ever replaced in place (this file never
does that, so every generation is `0`).

Between `obj` and `endobj` sits a **dictionary**: a set of `/Key value` pairs
wrapped in `<< >>`. Object 1's dictionary, `<< /Type /Catalog /Pages 2 0 R >>`,
is the **document catalog** — the root of the whole document — and its
`/Pages` entry holds `2 0 R`, an **indirect reference**: "look up object 2,
generation 0." Indirect references are how PDF objects link to each other
without needing to be stored in any particular order. Following them traces
a chain: the Catalog (1) points to a Pages tree (2), which lists one Kid,
`3 0 R` — the Page (3). The Page's `/Contents 5 0 R` points to object 5, a
**stream** (a dictionary followed by raw bytes between `stream`/`endstream`,
with `/Length` giving the byte count) holding the content-stream program
that draws "Hello, world!". Object 4 is a font resource the page's
`/Resources` dictionary points at by name (`/F1`).

The important habit to build here: a reader never starts at the top of the
file and reads forward like a book. It starts at `/Root` — found via the
trailer, below — and walks references outward from there. Objects can appear
in any order; nothing but the reference graph determines how they relate.

## Cross-reference table

The `xref` section is a lookup table: for every object number, the exact byte
offset from the start of the file where that object's `N 0 obj` line begins.
`0 6` means "6 entries, starting at object 0." Each entry after that is
exactly 20 bytes wide, always in the form `NNNNNNNNNN GGGGG X` — a 10-digit
byte offset, a space, a 5-digit generation number, a space, a one-letter
flag, and a two-byte end-of-line sequence: 10 + 1 + 5 + 1 + 1 + 2 = 20 bytes
on the dot.

The flag is either `f` (free) or `n` (in use). Object 0 is always the head of
a linked list of free object slots and is always `f`; PDF writers rarely
reuse those slots, so in practice it just marks "object number 0 doesn't
exist." Every other entry here is `n`, meaning "this object number is live —
go read it at this offset."

Fixed-width entries look like a small detail, but they're the whole point of
having a table at all: a reader can compute the address of object *N*'s xref
entry directly (`start of table + N × 20 bytes`) without scanning anything.
That's what makes a PDF a random-access structure instead of one you parse
top to bottom — the next article in this section, on objects and the xref
table, covers how that index is built, what happens when its offsets go
stale, and how hybrid files carrying both a classic table and an xref stream
are handled.

## Trailer

The `trailer` dictionary is short — here just `/Size 6` (the object count,
one more than the highest object number) and `/Root 1 0 R` (an indirect
reference to the Catalog, object 1) — but it's where every reader begins.
`startxref` on the next line gives one more byte offset: where the `xref`
keyword itself starts, `408` in this file.

That last detail is why a real reader parses a PDF from the **end backward**,
not from the header forward: find `%%EOF` at the tail of the file, step back
to read `startxref`, jump to the byte offset it names to find the `xref`
table, read the trailer that follows it, and use `/Root` to locate the
Catalog. Only after that does the reader start following the reference chain
described above — Catalog → Pages → Page → Contents — to render anything.
The header is read once, mostly to check the version; it plays no part in
locating content.

## In better-pdf

better-pdf's parser is **strict by design** — it ["rejects broken structure
rather than guessing at it"](/better-pdf/reference/limitations/), so a file
whose header, xref table, or trailer doesn't match what's described above
fails to load instead of being patched up silently.

Encryption sits at load time, ahead of any of this structure: it's opt-in via
`PdfDocument.load(bytes, { password })`, which decrypts RC4, AES-128, or
AES-256-protected files; calling bare `load(bytes)` on an encrypted file
throws `EncryptedPdfError` rather than returning garbage objects (see
[Filling & flattening forms](/better-pdf/guides/filling-forms/)).

When the structure genuinely doesn't parse — a missing object, a malformed
dictionary, anything the format doesn't allow — that surfaces as a typed
`PdfCoreError`, with the underlying core's message preserved so you can see
exactly what was wrong (see [Errors](/better-pdf/reference/errors/)).

Next: [Objects & the xref table](/better-pdf/internals/objects-and-xref/).
