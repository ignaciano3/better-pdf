# Docs landing page & visual identity — design

**Date:** 2026-07-04
**Status:** Approved pending user review
**Phase:** 1 of 3 (2: "How PDFs work" section · 3: runnable in-browser examples)

## Goal

The docs site (Starlight, `docs/site/`) looks like a default Starlight install
and undersells the library. Phase 1 gives it a distinctive visual identity and
rebuilds the landing page as a developer-facing pitch. The identity is shared
with the commercial editor (better-pdf.netlify.app) so the two properties read
as one product family, while remaining separate codebases (editor is closed
source; docs are open source).

## Direction

"Raw internals, engineering-first" (chosen over "sleek dev-tool" and
"paper & print" mockups): dark-first, monospace-led, with real PDF syntax
(`obj` / `xref` / `trailer`) as ambient decorative texture. Sells spec-level
precision and pairs with the future "How PDFs work" section.

## Visual system

### Color tokens (all pairs WCAG AA-verified, ratios measured)

| Role | Dark (designed-first) | Light |
|---|---|---|
| Page background | `#101014` | `#ffffff` |
| Surface / card | `#17171d` | `#f7f7f9` |
| Border | `#2e2e38` | `#e4e4e7` |
| Body text | `#e4e4e7` (15.0:1) | `#1c1c22` |
| Muted text | `#8b8b96` (5.6:1) | `#5c5c66` |
| Accent text / links | `#60a5fa` (7.5:1) | `#2563eb` (5.2:1) |
| Button fill | `#2563eb` + white text (5.2:1) | same |

Rules derived from the contrast audit:

- `#2563eb` is **never used as text on dark backgrounds** (3.67:1 — fails AA).
  On dark it is a fill only, with white text. Accent text on dark is `#60a5fa`.
- Muted text is never darker than `#8b8b96` on `#101014` (the earlier mockup's
  `#6b7280` measured 3.93:1 and was rejected).
- The decorative PDF-syntax texture is intentionally below 3:1 (`#26262e` on
  `#101014`); it is `aria-hidden="true"` and `user-select: none` — decoration,
  not content.

### Typography

- **IBM Plex Mono** — headings, nav, stats, accents (the "terminal" voice).
- **IBM Plex Sans** — body text.
- Same faces as the editor. Self-hosted via `@fontsource/ibm-plex-mono` and
  `@fontsource/ibm-plex-sans` (no Google Fonts request). System-font fallback
  stacks declared.

### Theme behavior

Starlight's light/dark toggle stays. Dark is the designed-first showcase; light
uses the same token roles on white. The syntax texture is theme-aware (a light
equivalent below 3:1 against white).

## Landing page (`src/content/docs/index.mdx`, splash template)

Seven sections, top to bottom, replacing the current six-card feature grid.
Rationale: the proof bar and code tabs *show* what the cards only claimed.

1. **Hero** — headline "Understands PDFs down to the byte." + subline
   (maintained pdf-lib alternative; Rust/WASM core; TypeScript API; Node &
   browser). CTAs: **Get started** (blue fill) and `$ npm i better-pdf`
   (copy-on-click). Faint PDF-syntax texture behind.
2. **Proof bar** — 4–7× faster fills · up to 186× faster no-op round-trip ·
   0 runtime deps · up to 45% smaller output (11.3 KB vs 20.7 KB, text-heavy
   default-settings scenario from `reference/benchmarks`). Each stat links to
   the benchmarks page. Numbers must match the benchmarks page; update together.
3. **Code tabs** — one realistic snippet per tab: Fill & flatten · Generate ·
   Typed forms · Sign. Static in this phase; the component boundary is designed
   so phase 3 can make tabs runnable without touching the page.
4. **vs pdf-lib** — honest comparison table (maintained vs archived, speed,
   strictness, incremental append-only saves, typed forms, output size), ending
   with a link to the migration guide.
5. **Editor showcase** — cross-promo band: "Not a developer? Use the editor."
   Screenshot + link to better-pdf.netlify.app. Doubles as social proof.
6. **Internals teaser** — *hidden until phase 2 ships.* Component is built as
   part of the landing system but not rendered until the "How PDFs work"
   section exists.
7. **Final CTA + footer** — repeat install + Get started; links: GitHub, npm,
   changelog, editor, license.

## Site-wide restyle

- All theming via `src/styles/custom.css` token overrides — no Starlight fork.
- Expressive Code (code blocks) themed to the palette: `#17171d` surfaces,
  matching syntax colors.
- Sidebar/nav in Plex Mono; blue link system throughout.
- Logo: keep the current mark, recolor to `#2563eb`.

## Component structure

New components in `docs/site/src/components/landing/`:

| Component | Responsibility |
|---|---|
| `Hero.astro` | Headline, CTAs, copy-to-clipboard install, texture |
| `ProofBar.astro` | Stat tiles + links (numbers passed as props) |
| `CodeTabs.astro` | Tabbed static snippets (phase-3 seam: swap internals) |
| `CompareTable.astro` | vs pdf-lib table |
| `EditorPromo.astro` | Editor cross-promo band |
| `FooterCta.astro` | Final CTA |

Each is self-contained (own scoped styles, props-driven) so it can be
understood, changed, and later upgraded independently.

## Accessibility & quality bar

- Every text/background pair ≥ 4.5:1 (table above; re-verify any new pair).
- Decorative texture: `aria-hidden`, below-3:1 by design, non-selectable.
- `prefers-reduced-motion` respected for any animation.
- Visible keyboard focus rings in accent blue.
- Copy-to-clipboard button has an accessible label and non-visual feedback.

## Verification

- `bun run build` (`astro build`) passes in `docs/site/`.
- Manual check of landing + one guide page + one API page in both themes.
- Lighthouse accessibility ≥ 95 on the landing page.
- Stats on the landing page cross-checked against `reference/benchmarks.md`.

## Out of scope

- "How PDFs work" section (phase 2 — agreed worth doing: 4–6 focused pages).
- Runnable in-browser examples (phase 3 — no conflict with the paid editor,
  which targets end users, not developers).
- Any change to the editor site; unifying the two sites (ruled out: closed vs
  open source).
- Logo redesign beyond recoloring.
