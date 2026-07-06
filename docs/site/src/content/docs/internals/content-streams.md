---
title: Content streams & operators
description: Every mark on a PDF page comes from a content stream — a tiny postfix language of operators like Tj, re, and cm. Here is a page painted instruction by instruction.
---

Open a PDF page and there's no text box, no rectangle object, no "shape" in
the sense a drawing program would use. A page doesn't *contain* text and
shapes — it contains a small program, a flat sequence of instructions that
paint text and shapes when a viewer runs them in order, top to bottom, once.
The anatomy article's `hello.pdf` had one such program sitting in object 5;
this article opens up what one looks like.

## A page, painted

```text title="A content stream: operands first, operator last"
q                              % push graphics state
0.9 0.9 0.95 rg                % set fill color (RGB)
72 700 200 40 re               % rectangle: x y width height
f                              % fill it
Q                              % pop graphics state

BT                             % begin text object
/F1 24 Tf                      % font F1 (from /Resources), 24 pt
1 0 0 1 80 712 Tm              % text matrix: place the cursor
0.1 0.2 0.8 rg                 % fill color for the glyphs
(Hello, world!) Tj             % paint the string
ET                             % end text object
```

Read it as a short story: save the current drawing state, switch to a pale
lavender fill, describe a rectangle, paint it, restore the state. Then start
a text run, pick a font and size, place the cursor, switch to a blue fill,
and paint one string of glyphs at that cursor position. Nothing in this file
says "there's a box and a heading" the way a word processor's document model
would — it says "do these seven things, in this order."

## Postfix, like a calculator

Every line follows the same shape: zero or more **operands**, then one
**operator**. `72 700 200 40 re` pushes four numbers and then calls `re`,
which pops them off and constructs a rectangle path from them. `(Hello,
world!) Tj` pushes one string and calls `Tj`, which pops it and paints it at
the current text position. This is a postfix (reverse Polish) notation, the
same idea as an old calculator where you enter `3 4 +` instead of `3 + 4`:
operands accumulate on an implicit stack, and the operator at the end of the
line is what actually does something with them.

There's no `if`, no loop, no variable to assign and reuse later, no function
to call — a content stream isn't a general-purpose programming language, it's
closer to a strip of paper tape a plotter arm reads and obeys one instruction
at a time. Two-letter or one-letter mnemonics (`re`, `f`, `Tj`, `Tf`) stand in
for "construct a rectangle," "fill the current path," "show this text,"
"select this font." The entire rendering model, no matter how visually
complex the page, reduces to that same pair: a **state machine** that
remembers things like current color and current font, plus a stream of
**painting commands** that read and mutate that state.

## The graphics state

`q` and `Q` are the bookends: `q` pushes a copy of the current graphics state
(fill color, line width, the active transform, and more) onto a stack, and
`Q` pops it back off, discarding whatever changed since the matching `q`.
That's why the rectangle in the example is wrapped in its own `q ... Q` pair
— the lavender fill color it sets only lives inside that bracket, so the
text that follows isn't accidentally painted lavender too. Nesting `q`/`Q`
freely is how a page can draw one self-contained element without it leaking
state into the next one.

The operator that does the heaviest lifting on that state is `cm`
(concatenate matrix): it multiplies a 2D affine transform into the current
transformation matrix, so everything drawn afterward is translated, scaled,
or rotated by it. `Tm`, used in the example (`1 0 0 1 80 712 Tm`), is the
same idea scoped to text — it sets the text matrix directly rather than
composing with what came before, placing the next glyph's origin at
`(80, 712)`.

Both operators work in the same coordinate space, and it's one worth
internalizing before it trips you up: the origin sits at the page's
**bottom-left corner**, with y increasing **upward**. A y of `712` on a
792-point-tall page is near the top, not near the bottom — the opposite of
the top-left, y-down convention used by CSS, `<canvas>`, and most image
formats. This is the same convention the drawing guides use when they talk
about placing text and shapes with `x`/`y` coordinates, so a mental model
built here transfers directly: up is positive, and the page's own height
determines what counts as "near the top."

## Where streams live

A content stream doesn't float free in the file — it's referenced the same
way every other piece of a PDF is, through the object graph the previous
article walked. The Page object's `/Contents` entry is an indirect reference
to the stream object holding this program (object 5 in `hello.pdf`, pointed
at by `/Contents 5 0 R`); a page can also have `/Contents` as an *array* of
several stream objects, concatenated in order and treated as one continuous
program. Names used inside the stream, like `/F1` in `/F1 24 Tf`, aren't
resolved by searching the file — they're looked up in the Page's
`/Resources` dictionary, which maps short names to the actual objects they
mean (`/Font << /F1 4 0 R >>` in the anatomy example points `/F1` at the
Helvetica font object). Images, other fonts, and reusable drawing fragments
(Form XObjects) are found the same way: a name in the stream, resolved
through `/Resources`, landing on an indirect reference into the object
graph.

## In better-pdf

`page.drawText()`, shapes, and images compile down to exactly these
operators ([Generating & drawing](/better-pdf/guides/generating/)).

Form-field appearances are content streams too: "appearances are generated
on fill, so flattening works on PDFs where pdf-lib throws `Unexpected N
type: undefined`" ([Filling & flattening forms](/better-pdf/guides/filling-forms/)).

Flattening "turns the field's current appearance into normal page content
and removes the interactive field"
([PdfForm.flattenField](/better-pdf/api-reference/classes/pdfform/)).

Next: [Forms](/better-pdf/internals/forms/).
