---
title: Incremental updates
description: PDFs can be edited by appending — a new body section, a new xref, a new trailer — while every original byte stays put. This is why signed documents can be amended without breaking the signature.
---

Most file formats have exactly one way to save: read the whole thing into
memory, apply the change, and write a fresh file from byte zero. Whatever was
on disk before is gone; only the new version remains. PDF supports that mode
too, but it also supports a second one that most formats don't: an update can
be **appended** to the end of the existing file instead of replacing it. The
previous article showed a complete six-object `hello.pdf`, ending with
`%%EOF` at byte 408's `xref`. Here's that same file after one incremental
save changes the page's text.

## An update is an appendix

```text title="hello.pdf after one incremental save (offsets illustrative)"
%PDF-1.7
… original objects 1–5, xref, trailer …
startxref
408
%%EOF                          % ← original file ends here; not one byte above changes

5 0 obj                        % UPDATE: a NEW version of object 5, appended
<< /Length 47 >>
stream
BT /F1 24 Tf 72 770 Td (Hello, appended!) Tj ET
endstream
endobj

xref                           % a SECOND xref, covering only what changed
0 1
0000000000 65535 f
5 1                            % subsection: 1 entry, starting at object 5
0000000531 00000 n             % the new object 5 lives in the appendix
trailer
<< /Size 6 /Root 1 0 R
   /Prev 408 >>                % ← chain to the PREVIOUS xref
startxref
531
%%EOF
```

Nothing before the first `%%EOF` moved. The original header, the original
objects 1 through 5, the original xref table, the original trailer — every
byte is exactly where it was. What's new is tacked on afterward: a fresh
copy of object 5 with different content, a second xref table that lists only
the objects this update touched, and a second trailer whose `/Prev` field
points back at the offset of the first xref. The file now has two revisions
layered on top of each other, and the second one only had to describe what
changed.

## How readers resolve it

A reader parsing this file still starts at the end, the same way described
in the anatomy article: find the last `%%EOF`, read the `startxref` above
it, and jump to the xref table it names. That lands on the *second* xref
table here — the one covering only object 5 — not the original. Its trailer
carries `/Prev 408`, telling the reader "there's an older xref at this
offset; consult it for anything I didn't mention." The reader follows that
chain backward, table by table, until it has a complete offset for every
object number.

When the same object number shows up in more than one table, the newest
entry wins — it's exactly like a dictionary where a later key write shadows
an earlier one. So a lookup for object 5 resolves to the appended copy at
offset 531, not the original at whatever offset it lived at in the first
table. That original object 5 is still sitting in the file, byte for byte,
between the two `%%EOF` markers — it simply isn't reachable anymore, because
no live xref entry points at it and nothing else in the document refers to
it directly. Superseded, not deleted.

## Why this matters for signatures

A digital signature over a PDF doesn't sign "the file" as an amorphous blob;
it signs a specific, enumerated **byte range** — typically everything except
a placeholder gap reserved for the signature value itself — computed at the
moment the signature was applied. As long as those exact bytes never change,
the signature stays verifiable no matter what happens afterward.

That's precisely the property an incremental update preserves. Because the
first `%%EOF` and everything before it are untouched, the byte range a
signature covered when it was created is still bit-for-bit identical after
the append. A viewer can add a second signature, or a reader can fill in
one more form field, using an incremental update, and the original
signature's byte range is never disturbed — so it still validates. This is
the mechanism that lets a contract carry two independent signatures applied
at different times by different parties, each verifiable independently,
without either signer's bytes ever being rewritten.

## The cost

Appending is cheap to write but not free to keep doing. Every incremental
save adds a full copy of whatever it touches — a form field's new
appearance, a page's new content stream — even when only a few bytes of the
underlying value actually changed. Superseded objects don't get reclaimed;
they linger in the file, unreachable but still occupying disk space, xref
tables pile up in a chain that grows longer with every save, and a document
that's been incrementally updated many times can end up noticeably larger
than one written from scratch with the same final content. A full rewrite —
starting from byte zero rather than appending — is the way to reclaim that
space, discarding every superseded object and writing one clean xref. The
[compression article](/better-pdf/internals/compression/) in this
section picks up that trade-off from the other side: how object streams
shrink a full rewrite even further.

## In better-pdf

On a loaded document, `save()` is an append-only incremental update, and it
"always starts from the originally loaded bytes — calling it twice returns
the same result" ([API](/better-pdf/reference/api/)).

"The original revision's bytes are preserved, so existing digital signatures
on it stay valid" ([API](/better-pdf/reference/api/)).

The `objectStreams` save option "applies only to full-document saves —
`create()`, `merge`, `assemble`, `copyPages`, `splitPages`. Incremental
(loaded-document) saves ignore it and remain append-only"
([Generating & drawing](/better-pdf/guides/generating/)).

Next: [Content streams](/better-pdf/internals/content-streams/).
