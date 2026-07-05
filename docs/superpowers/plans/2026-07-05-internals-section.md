# "How PDFs work" Internals Section Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a seven-article "How PDFs work" section (plus overview page) to the Starlight docs site, teaching PDF internals over annotated real bytes with a verified "In better-pdf" closer on each page, and un-gate the landing page's InternalsTeaser.

**Architecture:** Pure content + config: new pages in `docs/site/src/content/docs/internals/`, a new sidebar group in `astro.config.mjs` (grown one entry per task so every task builds green), one small Astro diagram component, and a two-line landing-page change. No new dependencies, no Starlight overrides beyond what phase 1 established.

**Tech Stack:** Astro 6 + Starlight 0.40 (existing), bun. All site commands run from `docs/site/`.

**Spec:** `docs/superpowers/specs/2026-07-05-internals-section-design.md`

**Testing model:** The site has no test framework. Every task's cycle is: `bun run build` exits 0 (catches broken slugs, bad MDX, missing imports), plus stated checks. Task 9 adds the browser + Lighthouse pass.

## Global Constraints

- Package name in any snippet: `@ignaciano3/better-pdf`. Internal links start with `/better-pdf/`.
- Every "In better-pdf" claim must match the repo's own docs. Each article task lists its claims WITH their supporting source file — implementers must not add claims beyond the listed ones, and prose must not contradict `reference/limitations.md`.
- **Forbidden claims (docs do not support them):** "lazy parsing" (the documented story is *strict by design; rejects broken structure rather than guessing*); "Identity-H" as a better-pdf implementation detail (docs say only "Type0/CIDFontType2 composite with a ToUnicode CMap"); cryptographic signing (visual appearances only); re-encryption or encrypting new documents (unsupported); "14 standard fonts shipped" (PDF defines 14; the library deliberately ships 12 — Symbol and ZapfDingbats omitted).
- Byte dumps must be structurally correct PDF syntax. Byte offsets inside examples are illustrative and every dump containing offsets carries the caption "(offsets illustrative)".
- Article length ~800–1200 words. No screenshots. At most one diagram per page (only Task 3 has one).
- Reuse phase-1 tokens (`--bp-*`, `--sl-*`); no new colors; `#2563eb` never as text on dark. Diagrams must render in both themes.
- Prose is written for developers who have never read the PDF spec: define every term at first use, prefer concrete bytes over abstractions.
- Frontmatter `description` on every page is a real sentence (SEO + link previews).

## File Structure

```
docs/site/src/content/docs/internals/
  index.md                 # Task 1 — overview + section map
  file-anatomy.md          # Task 2
  objects-and-xref.mdx     # Task 3 (only .mdx — imports the diagram)
  incremental-updates.md   # Task 4
  content-streams.md       # Task 5
  forms.md                 # Task 6
  fonts.md                 # Task 7
  compression.md           # Task 8
docs/site/src/components/internals/
  XrefDiagram.astro        # Task 3
docs/site/astro.config.mjs # Tasks 1–8 (sidebar group grows one entry per task)
docs/site/src/content/docs/index.mdx  # Task 9 (un-gate teaser)
```

**Writing the articles:** each article task below gives the complete skeleton — frontmatter, headings, every byte-dump code block verbatim, and the exact "In better-pdf" claims with links. The implementer authors the connective prose (intro, explanations between dumps, transitions) within that skeleton. Skeleton content is fixed; prose is yours, subject to the Global Constraints.

---

### Task 1: Sidebar group + overview page

**Files:**
- Modify: `docs/site/astro.config.mjs` (sidebar array, after the Examples group)
- Create: `docs/site/src/content/docs/internals/index.md`

**Interfaces:**
- Produces: the `How PDFs work` sidebar group that Tasks 2–8 each append one entry to; the `/better-pdf/internals/` route the landing teaser (Task 9) links to.

- [ ] **Step 1: Add the sidebar group**

In `docs/site/astro.config.mjs`, insert between the `Examples` group and the `Reference` group:

```js
{
	label: 'How PDFs work',
	items: [
		{ label: 'Overview', slug: 'internals' },
	],
},
```

- [ ] **Step 2: Create the overview page**

Create `docs/site/src/content/docs/internals/index.md`:

```markdown
---
title: How PDFs work
description: A short, byte-level tour of the PDF file format — objects, xref tables, incremental updates, content streams, forms, fonts, and compression — and how better-pdf puts each piece to work.
---
```

Then author the page body (~400–600 words — shorter than articles; this is a map, not a lesson):

1. Opening: what this section is — the PDF format explained over real bytes, written for developers who use PDFs but have never read the spec. Every page ends with how better-pdf embodies the concept.
2. A "Read in order or jump in" list of the seven articles, each with its link and a one-sentence hook. Exact links and titles:
   - `[Anatomy of a PDF file](/better-pdf/internals/file-anatomy/)` — the four parts of every PDF, shown on a complete 6-object file.
   - `[Objects & the xref table](/better-pdf/internals/objects-and-xref/)` — how a PDF is a random-access database of numbered objects.
   - `[Incremental updates](/better-pdf/internals/incremental-updates/)` — why editing a PDF can mean appending, never rewriting.
   - `[Content streams & operators](/better-pdf/internals/content-streams/)` — the tiny postfix language that paints every page.
   - `[AcroForms: how fields really work](/better-pdf/internals/forms/)` — field dictionaries, widgets, and what "flatten" actually does.
   - `[Fonts & text encoding](/better-pdf/internals/fonts/)` — why text extraction is hard and what a Type0 font is.
   - `[Compression & object streams](/better-pdf/internals/compression/)` — filters, FlateDecode, and packing objects into streams.
3. A closing note: byte offsets in examples are illustrative; the structures are real.

Note: the seven links point at pages created in Tasks 2–8. Starlight only validates sidebar slugs, not in-body links, so this builds green now; Task 9's verification clicks them.

- [ ] **Step 3: Build**

Run: `cd docs/site && bun run build`
Expected: exit 0, page count grows by 1 (94 pages).

- [ ] **Step 4: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/index.md
git commit -m "docs(site): internals section — sidebar group + overview page"
```

---

### Task 2: Anatomy of a PDF file

**Files:**
- Create: `docs/site/src/content/docs/internals/file-anatomy.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

**Interfaces:**
- Consumes: sidebar group from Task 1.
- Produces: the complete minimal PDF that later articles reference conceptually ("the file from the anatomy page").

- [ ] **Step 1: Add sidebar entry**

In the `How PDFs work` group's `items`, after `Overview`:

```js
{ label: 'File anatomy', slug: 'internals/file-anatomy' },
```

- [ ] **Step 2: Create the article**

Create `docs/site/src/content/docs/internals/file-anatomy.md` with this skeleton. Frontmatter:

```markdown
---
title: Anatomy of a PDF file
description: Every PDF is four parts — header, body, cross-reference table, and trailer. Here is a complete, working 6-object PDF, annotated line by line.
---
```

Headings and fixed content, in order:

**Intro** (prose): a PDF is not a black box — it is mostly readable text. This page walks a complete, minimal, valid PDF.

**## The whole file** — this dump, verbatim, with the caption "(offsets illustrative)":

````markdown
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
````

**Prose sections between/after the dump** (author these, ~150–250 words each):
- **## Header** — version line; the binary-marker comment.
- **## Body** — objects, indirect references (`2 0 R`), the Catalog → Pages → Page → Contents chain. Point out that the reader starts at `/Root`, not at the top of the file.
- **## Cross-reference table** — 20-byte fixed-width entries; `f` free vs `n` in-use; why fixed width enables random access (full story on the next page).
- **## Trailer** — `/Root`, `/Size`, `startxref`; a reader parses the file *backwards*: `%%EOF` → `startxref` → xref → trailer → `/Root`.

**## In better-pdf** — exactly these claims, as prose with these links:
- The parser is **strict by design** — it "rejects broken structure rather than guessing at it" ([Limitations](/better-pdf/reference/limitations/)). *(Source: `reference/limitations.md` Non-Goals; README Non-Goals.)*
- Encrypted files are opt-in at load: `PdfDocument.load(bytes, { password })` decrypts RC4/AES-128/AES-256; a bare `load(bytes)` on an encrypted file throws `EncryptedPdfError` ([Filling & flattening forms](/better-pdf/guides/filling-forms/)). *(Source: `guides/filling-forms.md`, `reference/limitations.md`.)*
- Malformed structure surfaces as a typed `PdfCoreError` with the core's message preserved ([Errors](/better-pdf/reference/errors/)). *(Source: `reference/errors.md`.)*

Close with a one-line pointer to the next article (`/better-pdf/internals/objects-and-xref/`).

- [ ] **Step 3: Build**

Run: `cd docs/site && bun run build` → exit 0 (95 pages).

- [ ] **Step 4: Accuracy self-check**

Re-read your prose against the Global Constraints' forbidden-claims list. Confirm every "In better-pdf" sentence appears in the claims list above — no additions.

- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/file-anatomy.md
git commit -m "docs(site): internals — anatomy of a PDF file"
```

---

### Task 3: Objects & the xref table (+ diagram component)

**Files:**
- Create: `docs/site/src/components/internals/XrefDiagram.astro`
- Create: `docs/site/src/content/docs/internals/objects-and-xref.mdx` (note: **.mdx**, it imports the diagram)
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

**Interfaces:**
- Consumes: `--bp-*` / `--sl-*` tokens (phase 1).
- Produces: `XrefDiagram.astro` (no props); the only diagram component in the section.

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'Objects & xref', slug: 'internals/objects-and-xref' },
```

- [ ] **Step 2: Create XrefDiagram.astro**

Create `docs/site/src/components/internals/XrefDiagram.astro` exactly:

```astro
---
// Xref-to-offset diagram for internals/objects-and-xref. Theme-token colors
// so it renders in both light and dark.
---

<svg
	viewBox="0 0 560 200"
	role="img"
	aria-label="Diagram: each cross-reference table entry stores the byte offset where its object begins, so a reader can jump straight to any object"
	class="xref-diagram"
>
	<defs>
		<marker id="xref-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
			<path d="M0,0 L10,5 L0,10 z" class="arrow-head"></path>
		</marker>
	</defs>
	<rect x="8" y="8" width="230" height="184" rx="6" class="box"></rect>
	<text x="24" y="34" class="label">xref</text>
	<text x="24" y="62" class="entry">0000000017 00000 n</text>
	<text x="24" y="92" class="entry">0000000066 00000 n</text>
	<text x="24" y="122" class="entry">0000000119 00000 n</text>
	<text x="24" y="176" class="muted">byte offsets</text>
	<rect x="330" y="8" width="222" height="184" rx="6" class="box"></rect>
	<text x="346" y="34" class="label">file bytes</text>
	<text x="346" y="62" class="entry accent">1 0 obj … endobj</text>
	<text x="346" y="92" class="entry accent">2 0 obj … endobj</text>
	<text x="346" y="122" class="entry accent">3 0 obj … endobj</text>
	<text x="346" y="176" class="muted">byte 17, 66, 119 …</text>
	<path d="M 218 58 C 280 58, 280 58, 340 58" class="arrow"></path>
	<path d="M 218 88 C 280 88, 280 88, 340 88" class="arrow"></path>
	<path d="M 218 118 C 280 118, 280 118, 340 118" class="arrow"></path>
</svg>

<style>
	.xref-diagram {
		display: block;
		max-width: 36rem;
		width: 100%;
		height: auto;
		margin: 1.5rem 0;
		font-family: var(--sl-font-mono);
	}
	.box { fill: var(--bp-surface); stroke: var(--bp-border); }
	.label { fill: var(--sl-color-white); font-size: 14px; font-weight: 600; }
	.entry { fill: var(--sl-color-gray-2); font-size: 12.5px; }
	.entry.accent { fill: var(--bp-accent-text); }
	.muted { fill: var(--sl-color-gray-3); font-size: 11px; font-family: var(--sl-font); }
	.arrow { stroke: var(--bp-accent-text); stroke-width: 1.5; fill: none; marker-end: url(#xref-arrow); }
	.arrow-head { fill: var(--bp-accent-text); }
</style>
```

- [ ] **Step 3: Create the article**

Create `docs/site/src/content/docs/internals/objects-and-xref.mdx`:

```mdx
---
title: Objects & the xref table
description: A PDF is a random-access database of numbered objects. The cross-reference table is its index — here is how readers jump straight to any object without scanning the file.
---

import XrefDiagram from '../../../components/internals/XrefDiagram.astro';
```

Fixed content, in order:

**Intro** (prose): the previous page showed the four parts; this one explains the two that make PDF a *database*, not a document stream.

**## The eight object types** — this dump verbatim:

````markdown
```text title="Every PDF value is one of eight types"
true false            % booleans
42  -7  3.14          % numbers
(Hello)               % literal string        <48656C6C6F>  % hex string
/Type /F1             % names (atoms, start with /)
[0 0 595 842]         % array
<< /Kind /Dict >>     % dictionary (the workhorse)
null                  % null
5 0 obj … endobj      % any value wrapped as a NUMBERED, reusable object
```
````

**## Indirect references** (prose + tiny inline examples): `5 0 R` means "the value of object 5, generation 0"; dictionaries reference each other by number, forming an object graph — this is why order in the file doesn't matter.

**## The xref table is the index** — prose explaining fixed 20-byte entries → O(1) seek; then render the diagram:

```mdx
<XrefDiagram />
```

**## What can go wrong** (prose, short): offsets that lie, truncated tables, hybrid files — and the two reader philosophies: rebuild-by-scanning (lenient) vs reject (strict).

**## In better-pdf** — exactly these claims:
- better-pdf takes the strict path: "the parser is strict by design and rejects broken structure rather than guessing at it" — corrupt xref data is an error, not a repair job ([Limitations](/better-pdf/reference/limitations/)). *(Source: `reference/limitations.md` Non-Goals.)*
- When the core rejects a file, you get a typed `PdfCoreError` carrying the core's original message ([Errors](/better-pdf/reference/errors/)). *(Source: `reference/errors.md`.)*

Close with a pointer to `/better-pdf/internals/incremental-updates/`.

- [ ] **Step 4: Build**

Run: `cd docs/site && bun run build` → exit 0 (96 pages).

- [ ] **Step 5: Accuracy self-check** (same check as Task 2 Step 4).

- [ ] **Step 6: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/components/internals/XrefDiagram.astro docs/site/src/content/docs/internals/objects-and-xref.mdx
git commit -m "docs(site): internals — objects and the xref table, with diagram"
```

---

### Task 4: Incremental updates

**Files:**
- Create: `docs/site/src/content/docs/internals/incremental-updates.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'Incremental updates', slug: 'internals/incremental-updates' },
```

- [ ] **Step 2: Create the article**

Frontmatter:

```markdown
---
title: Incremental updates
description: PDFs can be edited by appending — a new body section, a new xref, a new trailer — while every original byte stays put. This is why signed documents can be amended without breaking the signature.
---
```

Fixed content, in order:

**Intro** (prose): most formats rewrite the file on save. PDF has a second mode: append only.

**## An update is an appendix** — this dump verbatim, caption "(offsets illustrative)":

````markdown
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
````

**Prose sections:**
- **## How readers resolve it** — read the *last* trailer first; `/Prev` chains to older xrefs; the newest entry for an object number wins. The original object 5 is still in the file — unreachable, but intact.
- **## Why this matters for signatures** — a digital signature covers a byte range of the original revision; appending doesn't disturb those bytes, so prior signatures remain verifiable.
- **## The cost** — files grow on every save; superseded objects linger. (Full-rewrite saves reclaim the space — foreshadow the compression article's object streams.)

**## In better-pdf** — exactly these claims:
- `save()` is an append-only incremental update, and it "always starts from the originally loaded bytes — calling it twice returns the same result" ([API](/better-pdf/reference/api/)). *(Source: `reference/api.md`, `guides/filling-forms.md`.)*
- "The original revision's bytes are preserved, so existing digital signatures on it stay valid" ([API](/better-pdf/reference/api/)). *(Source: `reference/api.md`, README.)*
- The `objectStreams` save option "applies only to full-document saves — `create()`, `merge`, `assemble`, `copyPages`, `splitPages`. Incremental (loaded-document) saves ignore it and remain append-only" ([Generating & drawing](/better-pdf/guides/generating/)). *(Source: `guides/generating.mdx` Object streams.)*

Close with a pointer to `/better-pdf/internals/content-streams/`.

- [ ] **Step 3: Build** → exit 0 (97 pages).
- [ ] **Step 4: Accuracy self-check** (as Task 2).
- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/incremental-updates.md
git commit -m "docs(site): internals — incremental updates"
```

---

### Task 5: Content streams & operators

**Files:**
- Create: `docs/site/src/content/docs/internals/content-streams.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'Content streams', slug: 'internals/content-streams' },
```

- [ ] **Step 2: Create the article**

Frontmatter:

```markdown
---
title: Content streams & operators
description: Every mark on a PDF page comes from a content stream — a tiny postfix language of operators like Tj, re, and cm. Here is a page painted instruction by instruction.
---
```

Fixed content, in order:

**Intro** (prose): pages don't "contain" text and shapes; they contain a program that paints them.

**## A page, painted** — this dump verbatim:

````markdown
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
````

**Prose sections:**
- **## Postfix, like a calculator** — operands push, operator consumes; no variables, no loops; the whole model is "state machine + painting commands".
- **## The graphics state** — `q`/`Q` save/restore; `cm` transforms coordinates; explain the origin: **bottom-left, y increases upward** (link the coordinate convention the guides use).
- **## Where streams live** — `/Contents` on the page; `/Resources` maps names like `/F1` to font objects (connect back to the anatomy page's object graph).

**## In better-pdf** — exactly these claims:
- `page.drawText()`, shapes, and images compile down to exactly these operators ([Generating & drawing](/better-pdf/guides/generating/)). *(Source: `guides/generating.mdx` — the drawing API this page's operators correspond to.)*
- Form-field appearances are content streams too: "appearances are generated on fill, so flattening works on PDFs where pdf-lib throws `Unexpected N type: undefined`" ([Filling & flattening forms](/better-pdf/guides/filling-forms/)). *(Source: `guides/filling-forms.md`.)*
- Flattening "turns the field's current appearance into normal page content and removes the interactive field" ([PdfForm.flattenField](/better-pdf/api-reference/classes/pdfform/)). *(Source: `api-reference/classes/PdfForm.md`.)*

Close with a pointer to `/better-pdf/internals/forms/`.

- [ ] **Step 3: Build** → exit 0 (98 pages).
- [ ] **Step 4: Accuracy self-check** (as Task 2).
- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/content-streams.md
git commit -m "docs(site): internals — content streams and operators"
```

---

### Task 6: AcroForms — how fields really work

**Files:**
- Create: `docs/site/src/content/docs/internals/forms.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'AcroForms', slug: 'internals/forms' },
```

- [ ] **Step 2: Create the article**

Frontmatter:

```markdown
---
title: "AcroForms: how fields really work"
description: A form field is two things at once — a data node in a field tree and a widget annotation drawn on a page. Understanding the split explains filling, appearances, and flattening.
---
```

Fixed content, in order:

**Intro** (prose): "the form" is not a layer on top of the PDF; it is more objects in the same graph.

**## A field on disk** — this dump verbatim:

````markdown
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
````

**Prose sections:**
- **## Data vs pixels** — `/V` is the value; `/AP` is what viewers actually paint. A filled `/V` with a stale `/AP` is the classic "value doesn't show until you click the field" bug — appearances must be regenerated on fill.
- **## The field tree and names** — fields may nest via `/Kids`, and the full name is the chain of `/T` values joined with dots. So `applicant.name` can be one flat field *or* a `name` under an `applicant` parent — byte-different, name-identical.
- **## Flattening, precisely** — stamp the `/AP` stream into the page's content and delete the widget/field: the text stays visible, the interactivity is gone.

**## In better-pdf** — exactly these claims:
- Field names are surfaced fully qualified — "ancestor `/T` joined by `.`" — and on creation "each name is the field's literal, fully-qualified name; it is not split into a parent/child hierarchy on dots" ([Creating form fields](/better-pdf/guides/creating-form-fields/)). *(Source: `guides/creating-form-fields.mdx`, `api-reference/interfaces/FieldInfo.md`.)*
- `FieldInfo.widgets` has "one entry per widget annotation (page + position). Usually one; radio groups and fields repeated across pages have several" ([API](/better-pdf/reference/api/)). *(Source: `api-reference/interfaces/FieldInfo.md`.)*
- The typed-forms generator (`better-pdf-generate-types form.pdf src/form-types.ts`) turns this name model into compile-time safety: unknown names and invalid options become compile errors ([Typed forms](/better-pdf/guides/typed-forms/)). *(Source: `guides/typed-forms.md`.)*
- Unknown names at runtime throw `UnknownFieldError`; invalid choice values throw `InvalidOptionError` ([Errors](/better-pdf/reference/errors/)). *(Source: `reference/errors.md`.)*

Close with a pointer to `/better-pdf/internals/fonts/`.

- [ ] **Step 3: Build** → exit 0 (99 pages).
- [ ] **Step 4: Accuracy self-check** (as Task 2).
- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/forms.md
git commit -m "docs(site): internals — AcroForms"
```

---

### Task 7: Fonts & text encoding

**Files:**
- Create: `docs/site/src/content/docs/internals/fonts.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'Fonts & encoding', slug: 'internals/fonts' },
```

- [ ] **Step 2: Create the article**

Frontmatter:

```markdown
---
title: Fonts & text encoding
description: PDF strings are not text — they are glyph codes whose meaning depends on the font's encoding. From the standard 14 to Type0 composites, here is how PDFs turn bytes into visible words.
---
```

Fixed content, in order:

**Intro** (prose): `(Hello)` in a content stream only *happens* to be readable — the bytes are indexes into a font, not Unicode.

**## The simplest case: a standard font** — this dump verbatim:

````markdown
```text title="A Type1 standard font — no font file embedded"
4 0 obj
<< /Type /Font /Subtype /Type1
   /BaseFont /Helvetica >>     % viewer supplies the font — one of the "standard 14"
endobj
```
````

**Prose sections:**
- **## The standard 14 and WinAnsi** — the PDF spec guarantees 14 built-in fonts every viewer must render (Helvetica, Times, Courier families, Symbol, ZapfDingbats). Single-byte codes, typically WinAnsi encoding: fine for Western European text, hopeless for CJK or arbitrary Unicode.
- **## Composite (Type0) fonts** — this dump verbatim:

````markdown
```text title="A Type0 composite font — embedded, multi-byte, Unicode-mappable"
8 0 obj
<< /Type /Font /Subtype /Type0
   /BaseFont /NotoSansJP
   /Encoding /Identity-H       % 2-byte codes map straight to glyph IDs
   /DescendantFonts [9 0 R]    % → CIDFontType2 wrapping an embedded TrueType
   /ToUnicode 10 0 R >>        % CMap: glyph IDs back to Unicode (copy/paste, search)
endobj
```
````

  Explain: multi-byte codes; the descendant CIDFont carries the actual embedded font program; **`/ToUnicode` is why text is selectable and searchable** — without it, extraction is guesswork.
- **## Why text extraction is hard** (short): encoding varies per font, per page; subsetting renumbers glyphs; some producers omit `/ToUnicode` entirely.

**## In better-pdf** — exactly these claims (mind the 12-vs-14 nuance — the spec defines 14, the library ships 12):
- `StandardFonts` exposes 12 of the standard 14 — "Symbol and ZapfDingbats are intentionally omitted"; text in standard fonts "is limited to the WinAnsi charset" ([StandardFonts](/better-pdf/api-reference/enumerations/standardfonts/), [API](/better-pdf/reference/api/)). *(Source: `reference/api.md`, `api-reference/enumerations/StandardFonts.md`.)*
- `embedFont()` embeds any TTF/OTF "to render Unicode text — including CJK characters" as "a PDF Type0/CIDFontType2 composite with a ToUnicode CMap, so text is selectable and searchable" ([Generating & drawing](/better-pdf/guides/generating/)). *(Source: `guides/generating.mdx` Custom fonts.)*
- Current limitation: re-filling an embedded-font form field through the form API is not yet supported and throws; form-field values render with standard-14 fonts ([Limitations](/better-pdf/reference/limitations/)). *(Source: `reference/limitations.md`.)*

Close with a pointer to `/better-pdf/internals/compression/`.

- [ ] **Step 3: Build** → exit 0 (100 pages).
- [ ] **Step 4: Accuracy self-check** (as Task 2). Note the fine line in this article: `/Identity-H` appears in the **generic PDF example** (correct PDF teaching) but must never be stated as a better-pdf implementation detail — the library claim is only "Type0/CIDFontType2 with a ToUnicode CMap".
- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/fonts.md
git commit -m "docs(site): internals — fonts and text encoding"
```

---

### Task 8: Compression & object streams

**Files:**
- Create: `docs/site/src/content/docs/internals/compression.md`
- Modify: `docs/site/astro.config.mjs` (add sidebar entry)

- [ ] **Step 1: Add sidebar entry**

```js
{ label: 'Compression & object streams', slug: 'internals/compression' },
```

- [ ] **Step 2: Create the article**

Frontmatter:

```markdown
---
title: Compression & object streams
description: PDFs shrink in two ways — filters that deflate individual streams, and object streams that pack whole objects together. Here is what /FlateDecode and /ObjStm actually do.
---
```

Fixed content, in order:

**Intro** (prose): the anatomy page's file was all plain text; real PDFs mostly aren't. Two mechanisms, two different targets.

**## Filters: compressing stream data** — this dump verbatim:

````markdown
```text title="The same content stream, deflated"
5 0 obj
<< /Length 38                  % length of the COMPRESSED bytes
   /Filter /FlateDecode >>     % decompress with zlib/deflate before use
stream
x\x9c\x0bA\x11… (38 bytes of deflate data)
endstream
endobj
```
````

  Prose: filters apply per stream; text-heavy streams deflate dramatically; already-compressed payloads (JPEG images, font programs) don't — recompressing them wastes work.

**## Object streams: compressing the structure** — this dump verbatim:

````markdown
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
````

  Prose: dictionaries themselves are tiny but numerous; packing them into one deflated stream compresses the *structure*, not just the content. Requires cross-reference **streams** (the xref itself becomes a compressed object — mention briefly, one sentence, as the PDF 1.5 companion feature).

**## The trade-off** (short prose): object streams rewrite structure, so they're a property of *full-document* writes — they are fundamentally incompatible with the append-only editing model from the incremental-updates article.

**## In better-pdf** — exactly these claims:
- `compress` (on by default) deflates generated content/appearance/font streams and leaves already-compressed streams untouched; on incremental saves "only the newly appended section is compressed — the original revision's bytes are preserved" ([Generating & drawing](/better-pdf/guides/generating/), [API](/better-pdf/reference/api/)). *(Source: `guides/generating.mdx`, `reference/api.md`.)*
- `objectStreams` (off by default) "applies only to full-document saves — `create()`, `merge`, `assemble`, `copyPages`, `splitPages`. Incremental (loaded-document) saves ignore it and remain append-only, so existing signatures stay valid" ([Generating & drawing](/better-pdf/guides/generating/)). *(Source: `guides/generating.mdx` Object streams.)*
- The output-size effect is measured on the [Benchmarks](/better-pdf/reference/benchmarks/) page. *(Source: `reference/benchmarks.md` output-size table.)*

Close with a pointer back to the overview (`/better-pdf/internals/`) — this is the last article.

- [ ] **Step 3: Build** → exit 0 (101 pages).
- [ ] **Step 4: Accuracy self-check** (as Task 2).
- [ ] **Step 5: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/content/docs/internals/compression.md
git commit -m "docs(site): internals — compression and object streams"
```

---

### Task 9: Un-gate the landing teaser + full verification

**Files:**
- Modify: `docs/site/src/content/docs/index.mdx`

**Interfaces:**
- Consumes: `InternalsTeaser.astro` (phase 1, currently unimported); the `/better-pdf/internals/` route from Task 1.

- [ ] **Step 1: Render the teaser**

In `docs/site/src/content/docs/index.mdx`, add the import (with the other component imports):

```mdx
import InternalsTeaser from '../../components/landing/InternalsTeaser.astro';
```

and render it between `<EditorPromo />` and `<FooterCta />`:

```mdx
<EditorPromo />
<InternalsTeaser />
<FooterCta />
```

- [ ] **Step 2: Build**

Run: `cd docs/site && bun run build` → exit 0.

- [ ] **Step 3: Landing + section verification**

With `bun run preview` serving `dist`:
1. `curl -s http://localhost:4321/better-pdf/ | grep -c 'How PDFs actually work'` → ≥ 1 (teaser rendered).
2. Landing still has exactly one `<h1>`: `grep -c '<h1' dist/index.html` → 1.
3. Every internals page exists and links resolve: for each of the eight slugs (`internals/`, `internals/file-anatomy/`, `internals/objects-and-xref/`, `internals/incremental-updates/`, `internals/content-streams/`, `internals/forms/`, `internals/fonts/`, `internals/compression/`), `curl -s -o /dev/null -w "%{http_code}" http://localhost:4321/better-pdf/<slug>` → 200.
4. Spot-check the "In better-pdf" links: `grep -o 'href="/better-pdf/[^"]*"' dist/internals/*/index.html | sort -u`, confirm every target path exists under `dist/`.

- [ ] **Step 4: Both-themes browser pass (spec §Verification)**

In a browser (Playwright/Brave or manual) against the preview server, check in BOTH themes (toggle `data-theme`): the internals index, `objects-and-xref` (the SVG diagram must be legible in light and dark — token colors, no invisible strokes), and one more article. Also confirm the landing teaser band renders correctly in both themes.

- [ ] **Step 5: Lighthouse accessibility ≥ 95 on the internals index**

```bash
CHROME_PATH="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser" \
  npx lighthouse http://localhost:4321/better-pdf/internals/ --only-categories=accessibility --chrome-flags="--headless" --output=json --output-path=<scratchpad>/lh-internals.json --quiet
```

Extract `categories.accessibility.score` → required ≥ 0.95. Also run once against `/better-pdf/internals/objects-and-xref/` (the page with the SVG diagram) → ≥ 0.95. If below: the report's failing audits name the offending nodes — fix, rebuild, re-run. Kill the preview server when done.

- [ ] **Step 6: Commit**

```bash
git add docs/site/src/content/docs/index.mdx
git commit -m "docs(site): un-gate InternalsTeaser — internals section is live"
```
