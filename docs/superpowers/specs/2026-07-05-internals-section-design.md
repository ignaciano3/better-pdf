# "How PDFs work" internals section — design (phase 2)

**Date:** 2026-07-05
**Status:** Approved pending user review
**Phase:** 2 of 3 (1: landing & visual identity — shipped; 3: runnable in-browser examples)

## Goal

Add an educational "How PDFs work" section to the docs site (`docs/site/`,
Starlight): seven articles plus an index that teach PDF internals over
annotated real bytes, each ending with how better-pdf embodies the concept.
Un-gate the landing page's `InternalsTeaser` (built in phase 1, deliberately
unrendered) now that its `/better-pdf/internals/` target exists.

## Voice

PDF-first, tied to better-pdf: each page teaches the format properly
(real byte fragments, spec-level precision), then closes with a short
**"In better-pdf"** section connecting the concept to the library's behavior
and linking to the relevant guide/API pages. This extends the phase-1 brand
("Understands PDFs down to the byte") and differentiates rather than
duplicating generic PDF explainers.

## Pages

New content directory `docs/site/src/content/docs/internals/`, reading order:

| # | Title | Slug | "In better-pdf" closer |
|---|---|---|---|
| 0 | How PDFs work (overview) | `internals/` (index) | section map, who it's for |
| 1 | Anatomy of a PDF file | `internals/file-anatomy` | what `load()` actually reads first |
| 2 | Objects & the xref table | `internals/objects-and-xref` | lazy parsing; typed errors on malformed xref |
| 3 | Incremental updates | `internals/incremental-updates` | why `save()` is append-only & byte-preserving |
| 4 | Content streams & operators | `internals/content-streams` | `drawText`/drawing; flatten appearance streams |
| 5 | AcroForms: how fields really work | `internals/forms` | fill/flatten; typed forms |
| 6 | Fonts & text encoding | `internals/fonts` | StandardFonts vs `embedFont`; CJK/Type0 |
| 7 | Compression & object streams | `internals/compression` | `compress` + `objectStreams` save options |

## Navigation

- New sidebar group **"How PDFs work"** in `astro.config.mjs`, placed between
  **Examples** and **Reference** (conceptual deep-dive after practical
  material). Index page listed first inside the group.
- Landing page (`src/content/docs/index.mdx`): import and render
  `<InternalsTeaser />` between `<EditorPromo />` and `<FooterCta />` — the
  phase-1 spec's original slot 6. No changes to the teaser component beyond
  what rendering reveals (its link already points at `/better-pdf/internals/`).
- Each article ends with prev/next links (Starlight's built-in pagination
  covers this once the sidebar group exists; no custom component).

## Page anatomy (identical on all seven articles)

1. **Frontmatter:** `title`, `description` (real sentence, for SEO/teaser).
2. **Short intro** — what this page explains and why a developer using PDFs
   should care (2–4 sentences).
3. **The concept over annotated real bytes** — minimal hand-crafted PDF
   fragments in fenced code blocks, annotated with PDF's own `%` comment
   syntax plus Expressive Code line highlights/titles. This is the anchor of
   every page: the hero texture, now explained line by line.
4. **At most one small inline SVG diagram** per page, only where structure
   needs arrows (xref→offset mapping, object graph, field tree). Drawn with
   theme tokens (`currentColor` / `var(--sl-color-*)`, `var(--bp-*)`) so it
   reads correctly in both themes; `role="img"` with a meaningful
   `aria-label`.
5. **"In better-pdf"** closing section — how the library embodies the
   concept, with links to the relevant guide and API-reference pages.
6. Target length ~800–1200 words per article. No screenshots anywhere.

## Accuracy rules (binding)

- Every "In better-pdf" claim must be verifiable against the repo (README,
  guides, reference pages, or source). Implementers record the source for
  each claim; reviewers check them — same discipline as phase 1's
  stats-match-benchmarks rule.
- Byte dumps must be structurally correct PDF syntax. Byte offsets inside
  examples may be illustrative rather than exact, and are labeled as such
  wherever they appear.
- No invented API, no aspirational features, no capability claims the
  Limitations page contradicts.
- Package name in any snippet: `@ignaciano3/better-pdf`. Internal links start
  with `/better-pdf/`.

## Visual & accessibility system (inherited from phase 1)

- Uses the existing token system; no new colors. Accent text via
  `--bp-accent-text`; `#2563eb` never as text on dark.
- Diagrams and code blocks must render in both themes; decorative elements
  (if any) follow the phase-1 texture rules (`aria-hidden`, non-selectable).
- Prose is written for developers who have never read the PDF spec —
  precision without assuming prior format knowledge.

## Verification

- `bun run build` (from `docs/site/`) passes.
- Browser pass: index + at least two articles in both themes; landing page
  renders the teaser and its link resolves.
- Lighthouse accessibility ≥ 95 on `/better-pdf/internals/`.
- All internal links on new pages resolve (build catches broken Starlight
  slugs; spot-check the "In better-pdf" links).

## Out of scope

- Runnable/in-browser examples (phase 3 — CodeTabs seam, may later also make
  internals byte dumps interactive).
- Encryption internals and cryptographic signatures (the library renders
  signature *appearances* only; an internals page would overpromise).
- PDF 2.0-specific features; linearization; tagged-PDF/accessibility trees.
- Any change to the editor site.
- Regeneration tooling for byte dumps (dumps are hand-curated; revisit if
  they drift).
