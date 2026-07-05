# Docs Landing Page & Visual Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the Starlight docs site to the approved "engineering-dark + blue brand" identity and rebuild the landing page as a seven-section developer pitch.

**Architecture:** All theming happens through `docs/site/src/styles/custom.css` token overrides and Starlight's `expressiveCode.styleOverrides` — no Starlight fork. The landing page is rebuilt from six self-contained Astro components in `docs/site/src/components/landing/`, wired together by `src/content/docs/index.mdx` (splash template, custom sections instead of the frontmatter hero/card grid).

**Tech Stack:** Astro 6 + Starlight 0.40 (existing), `@fontsource/ibm-plex-sans` + `@fontsource/ibm-plex-mono` (new dev-facing deps of the docs site only), bun as package manager (site already has `bun.lock`).

**Spec:** `docs/superpowers/specs/2026-07-04-docs-landing-visual-identity-design.md`

**Testing model:** The docs site has no test framework. Every task's test cycle is: `bun run build` must pass (catches Astro/MDX/component errors), plus a stated manual check in `bun run dev`. Run all site commands from `docs/site/`.

## Global Constraints

- Package name in all copy and snippets: `@ignaciano3/better-pdf` (NOT `better-pdf` — that npm name is not ours).
- Site base path: `/better-pdf` — all internal links start with `/better-pdf/` (matches existing content pages).
- `#2563eb` is never used as text on dark backgrounds (3.67:1, fails AA). On dark it is a fill with white text only; accent text on dark is `#60a5fa`.
- Muted text on `#101014` is never darker than `#8b8b96`.
- Decorative PDF-syntax texture: `aria-hidden="true"`, `user-select: none`, intentionally < 3:1.
- Landing stats must match `src/content/docs/reference/benchmarks.md`: 5–8× faster fills, 186× no-op round-trip, 0 runtime dependencies, up to 45% smaller output (11.3 KB vs 20.7 KB).
- Do not fork or eject Starlight components; theme via CSS custom properties and documented config options only.
- Keep the existing light/dark toggle working; every change must look right in both themes.

---

### Task 1: Design tokens & fonts

**Files:**
- Modify: `docs/site/package.json` (via `bun add`)
- Modify: `docs/site/astro.config.mjs` (customCss array)
- Rewrite: `docs/site/src/styles/custom.css`

**Interfaces:**
- Produces: CSS custom properties consumed by every later task — `--sl-*` Starlight tokens plus landing tokens `--bp-accent-text`, `--bp-accent-fill`, `--bp-texture`, `--bp-surface`, `--bp-border`. Font stacks `var(--sl-font)` (IBM Plex Sans) and `var(--sl-font-mono)` (IBM Plex Mono).

- [ ] **Step 1: Install fonts**

```bash
cd docs/site && bun add @fontsource/ibm-plex-sans @fontsource/ibm-plex-mono
```

- [ ] **Step 2: Register font CSS in astro.config.mjs**

In the `starlight({ ... })` options, add `customCss` (there is currently a `customCss: ['./src/styles/custom.css']` entry — replace it):

```js
customCss: [
	'@fontsource/ibm-plex-sans/400.css',
	'@fontsource/ibm-plex-sans/500.css',
	'@fontsource/ibm-plex-sans/600.css',
	'@fontsource/ibm-plex-mono/400.css',
	'@fontsource/ibm-plex-mono/500.css',
	'@fontsource/ibm-plex-mono/600.css',
	'./src/styles/custom.css',
],
```

- [ ] **Step 3: Rewrite custom.css with the full token system**

Replace the entire contents of `docs/site/src/styles/custom.css`:

```css
/* better-pdf — engineering-dark identity, blue brand family.
   Contrast rules (measured, WCAG):
   - #2563eb is a FILL on dark (3.67:1 as text — fails AA); accent text on dark is #60a5fa (7.5:1).
   - Muted text on #101014 is never darker than #8b8b96 (5.6:1). */

:root {
	--sl-font: 'IBM Plex Sans', ui-sans-serif, system-ui, sans-serif;
	--sl-font-mono: 'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, monospace;

	/* Dark (designed-first) */
	--sl-color-accent-low: #16233f;
	--sl-color-accent: #2563eb;
	--sl-color-accent-high: #60a5fa;

	--sl-color-white: #e4e4e7;
	--sl-color-gray-1: #d4d4d8;
	--sl-color-gray-2: #b8b8c0;
	--sl-color-gray-3: #8b8b96;
	--sl-color-gray-4: #55555f;
	--sl-color-gray-5: #2e2e38;
	--sl-color-gray-6: #17171d;
	--sl-color-black: #101014;

	/* Landing tokens */
	--bp-accent-text: #60a5fa;
	--bp-accent-fill: #2563eb;
	--bp-surface: #17171d;
	--bp-border: #2e2e38;
	--bp-texture: #26262e;
}

:root[data-theme='light'] {
	--sl-color-accent-low: #dbeafe;
	--sl-color-accent: #2563eb;
	--sl-color-accent-high: #1e40af;

	--sl-color-white: #1c1c22;
	--sl-color-gray-1: #2a2a32;
	--sl-color-gray-2: #3f3f49;
	--sl-color-gray-3: #5c5c66;
	--sl-color-gray-4: #8b8b96;
	--sl-color-gray-5: #d8d8de;
	--sl-color-gray-6: #f7f7f9;
	--sl-color-gray-7: #f2f2f5;
	--sl-color-black: #ffffff;

	--bp-accent-text: #2563eb;
	--bp-accent-fill: #2563eb;
	--bp-surface: #f7f7f9;
	--bp-border: #e4e4e7;
	--bp-texture: #ececf0;
}

/* Monospace voice: headings, site title, sidebar nav */
h1, h2, h3, h4, h5, h6,
.site-title,
.sidebar-pane {
	font-family: var(--sl-font-mono);
}

/* Visible keyboard focus in accent blue */
:focus-visible {
	outline: 2px solid var(--bp-accent-text);
	outline-offset: 2px;
}

/* Screen-reader-only utility (used by landing components) */
.sr-only {
	position: absolute;
	width: 1px;
	height: 1px;
	padding: 0;
	margin: -1px;
	overflow: hidden;
	clip: rect(0, 0, 0, 0);
	white-space: nowrap;
	border: 0;
}

@media (prefers-reduced-motion: reduce) {
	*, *::before, *::after {
		animation-duration: 0.01ms !important;
		transition-duration: 0.01ms !important;
	}
}
```

- [ ] **Step 4: Build**

Run: `cd docs/site && bun run build`
Expected: exits 0 (Starlight + TypeDoc pages generate without errors).

- [ ] **Step 5: Manual check**

Run `bun run dev`, open the site. Verify: Plex Sans body / Plex Mono headings and sidebar; dark page background `#101014`; links readable in both themes (toggle); no unstyled-font flash on reload.

- [ ] **Step 6: Commit**

```bash
git add docs/site/package.json docs/site/bun.lock docs/site/astro.config.mjs docs/site/src/styles/custom.css
git commit -m "docs(site): IBM Plex fonts + engineering-dark blue token system"
```

---

### Task 2: Code-block theming & logo recolor

**Files:**
- Modify: `docs/site/astro.config.mjs` (expressiveCode option)
- Modify: `docs/site/src/assets/logo.svg` (line 2 fill)

**Interfaces:**
- Consumes: gray tokens from Task 1.
- Produces: code blocks on `--sl-color-gray-6` surfaces site-wide; blue logo.

- [ ] **Step 1: Add expressiveCode styleOverrides**

In `starlight({ ... })` options (sibling of `customCss`), add:

```js
expressiveCode: {
	styleOverrides: {
		borderColor: 'var(--sl-color-gray-5)',
		borderRadius: '0.5rem',
		codeBackground: 'var(--sl-color-gray-6)',
		codeFontFamily: 'var(--sl-font-mono)',
		uiFontFamily: 'var(--sl-font-mono)',
		frames: {
			editorActiveTabBackground: 'var(--sl-color-gray-6)',
			editorTabBarBackground: 'var(--sl-color-black)',
			terminalBackground: 'var(--sl-color-gray-6)',
			terminalTitlebarBackground: 'var(--sl-color-black)',
		},
	},
},
```

- [ ] **Step 2: Recolor logo**

In `docs/site/src/assets/logo.svg`, change line 2:

```svg
  <rect width="32" height="32" rx="7" fill="#2563eb"/>
```

(was `fill="#6366f1"`).

- [ ] **Step 3: Build & check**

Run: `bun run build` → exits 0. In `bun run dev`: open a guide page (e.g. `/better-pdf/getting-started/quickstart/`), confirm code blocks sit on the dark surface with the gray-5 border in dark mode and `#f7f7f9` in light mode; header logo is blue.

- [ ] **Step 4: Commit**

```bash
git add docs/site/astro.config.mjs docs/site/src/assets/logo.svg
git commit -m "docs(site): theme code blocks to palette, recolor logo to brand blue"
```

---

### Task 3: Hero component

**Files:**
- Create: `docs/site/src/components/landing/Hero.astro`

**Interfaces:**
- Consumes: `--bp-*` tokens, `--sl-font-mono`, `.sr-only` (Task 1).
- Produces: `<Hero />`, no props. Used by Task 7's `index.mdx`.

- [ ] **Step 1: Create Hero.astro**

```astro
---
// Landing hero: headline, CTAs, copy-to-clipboard install command, and a
// decorative PDF-syntax texture (aria-hidden, deliberately below 3:1).
const install = 'npm i @ignaciano3/better-pdf';
---

<section class="hero-bp">
	<pre class="texture" aria-hidden="true">%PDF-1.7
3 0 obj
&lt;&lt; /Type /Page /Annots [12 0 R] &gt;&gt;
endobj
xref
0 21
0000000000 65535 f
0000000017 00000 n
trailer &lt;&lt; /Root 1 0 R /Prev 41230 &gt;&gt;
startxref
%%EOF</pre>
	<div class="inner">
		<h1>&gt; Understands PDFs<br />&nbsp;&nbsp;down to the <span class="accent">byte</span>.</h1>
		<p class="sub">
			The maintained, fast alternative to pdf-lib — fill, flatten, sign, and
			generate PDFs. Rust/WebAssembly core, TypeScript API, Node &amp; browser.
		</p>
		<div class="ctas">
			<a class="btn" href="/better-pdf/getting-started/introduction/">Get started</a>
			<button class="install" type="button" data-install={install}>
				<span aria-hidden="true">$&nbsp;</span>{install}
			</button>
			<span class="sr-only" role="status" data-copy-status></span>
		</div>
	</div>
</section>

<script>
	const btn = document.querySelector<HTMLButtonElement>('.install');
	const status = document.querySelector<HTMLElement>('[data-copy-status]');
	btn?.addEventListener('click', async () => {
		await navigator.clipboard.writeText(btn.dataset.install ?? '');
		btn.classList.add('copied');
		if (status) status.textContent = 'Install command copied to clipboard';
		setTimeout(() => {
			btn.classList.remove('copied');
			if (status) status.textContent = '';
		}, 2000);
	});
</script>

<style>
	.hero-bp {
		position: relative;
		overflow: hidden;
		padding: clamp(3rem, 10vw, 6rem) 0 clamp(2rem, 6vw, 4rem);
		font-family: var(--sl-font-mono);
	}
	.texture {
		position: absolute;
		inset: 0;
		margin: 0;
		padding: 1rem;
		color: var(--bp-texture);
		background: none;
		border: none;
		font-size: 0.8rem;
		line-height: 1.9;
		user-select: none;
		pointer-events: none;
	}
	.inner { position: relative; }
	h1 {
		font-size: clamp(1.8rem, 5vw, 3.2rem);
		line-height: 1.15;
		color: var(--sl-color-white);
	}
	.accent { color: var(--bp-accent-text); }
	.sub {
		font-family: var(--sl-font);
		color: var(--sl-color-gray-3);
		max-width: 40rem;
		margin-top: 1rem;
		font-size: 1.1rem;
	}
	.ctas {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-top: 1.75rem;
		align-items: center;
	}
	.btn {
		background: var(--bp-accent-fill);
		color: #ffffff;
		font-family: var(--sl-font);
		font-weight: 600;
		padding: 0.6rem 1.4rem;
		border-radius: 0.375rem;
		text-decoration: none;
	}
	.btn:hover { background: #1d4ed8; }
	.install {
		background: var(--bp-surface);
		border: 1px solid var(--bp-border);
		color: var(--bp-accent-text);
		font-family: var(--sl-font-mono);
		font-size: 0.95rem;
		padding: 0.6rem 1.1rem;
		border-radius: 0.375rem;
		cursor: pointer;
	}
	.install.copied::after {
		content: ' ✓';
	}
</style>
```

- [ ] **Step 2: Build**

Run: `bun run build` → exits 0. (Component is not rendered anywhere yet; the build verifies it parses.)

- [ ] **Step 3: Commit**

```bash
git add docs/site/src/components/landing/Hero.astro
git commit -m "docs(site): landing Hero component"
```

---

### Task 4: ProofBar & CompareTable components

**Files:**
- Create: `docs/site/src/components/landing/ProofBar.astro`
- Create: `docs/site/src/components/landing/CompareTable.astro`

**Interfaces:**
- Consumes: `--bp-*` tokens (Task 1).
- Produces: `<ProofBar />` and `<CompareTable />`, no props (stats hardcoded next to a comment pointing at the benchmarks page they must stay in sync with).

- [ ] **Step 1: Create ProofBar.astro**

```astro
---
// Benchmark stat tiles. Numbers MUST match
// src/content/docs/reference/benchmarks.md — update both together.
const stats = [
	{ value: '5–8×', label: 'faster form fills than pdf-lib' },
	{ value: '186×', label: 'faster no-op round-trip' },
	{ value: '0', label: 'runtime dependencies' },
	{ value: '45%', label: 'smaller output (text-heavy, defaults)' },
];
---

<section class="proof" aria-label="Benchmark highlights">
	{stats.map((s) => (
		<a class="stat" href="/better-pdf/reference/benchmarks/">
			<span class="value">{s.value}</span>
			<span class="label">{s.label}</span>
		</a>
	))}
</section>

<style>
	.proof {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
		gap: 1px;
		background: var(--bp-border);
		border: 1px solid var(--bp-border);
		border-radius: 0.5rem;
		overflow: hidden;
		margin: 2rem 0;
	}
	.stat {
		background: var(--bp-surface);
		padding: 1.25rem 1rem;
		text-decoration: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.stat:hover .value { text-decoration: underline; }
	.value {
		font-family: var(--sl-font-mono);
		font-weight: 600;
		font-size: 1.6rem;
		color: var(--bp-accent-text);
	}
	.label {
		font-family: var(--sl-font);
		font-size: 0.85rem;
		color: var(--sl-color-gray-3);
	}
</style>
```

- [ ] **Step 2: Create CompareTable.astro**

```astro
---
// Honest comparison vs pdf-lib. Claims verified against README and
// reference/benchmarks.md — keep factual, no strawmen.
const rows = [
	['Maintenance', 'Actively maintained', 'Last release 2021'],
	['Form fills', '5–8× faster', 'baseline'],
	['Unknown fields / bad options', 'Typed errors', 'Silent or generic throw'],
	['Saving', 'Incremental, append-only (original bytes preserved)', 'Full rewrite'],
	['Typed form schemas', 'Generated module — wrong names are compile errors', 'None'],
	['Output size (defaults, text-heavy)', '11.3 KB', '20.7 KB'],
];
---

<section class="compare">
	<h2 id="vs-pdf-lib">vs pdf-lib</h2>
	<table>
		<thead>
			<tr><th scope="col"></th><th scope="col">better-pdf</th><th scope="col">pdf-lib</th></tr>
		</thead>
		<tbody>
			{rows.map(([k, us, them]) => (
				<tr><th scope="row">{k}</th><td class="us">{us}</td><td>{them}</td></tr>
			))}
		</tbody>
	</table>
	<p class="migrate">
		Coming from pdf-lib? Most code ports in minutes —
		<a href="/better-pdf/migrating/from-pdf-lib/">migration guide</a>.
	</p>
</section>

<style>
	.compare { margin: 3rem 0; }
	table { width: 100%; }
	th[scope='row'] {
		font-family: var(--sl-font);
		font-weight: 500;
		color: var(--sl-color-gray-3);
		text-align: left;
	}
	.us { color: var(--sl-color-white); font-weight: 500; }
	.migrate { color: var(--sl-color-gray-3); }
	.migrate a { color: var(--bp-accent-text); }
</style>
```

- [ ] **Step 3: Build**

Run: `bun run build` → exits 0.

- [ ] **Step 4: Commit**

```bash
git add docs/site/src/components/landing/ProofBar.astro docs/site/src/components/landing/CompareTable.astro
git commit -m "docs(site): landing ProofBar and CompareTable components"
```

---

### Task 5: CodeTabs component

**Files:**
- Create: `docs/site/src/components/landing/CodeTabs.astro`

**Interfaces:**
- Consumes: Starlight's `Tabs`, `TabItem`, `Code` components; Expressive Code theming (Task 2).
- Produces: `<CodeTabs />`, no props. Phase-3 seam: to make tabs runnable later, only this component's internals change.

- [ ] **Step 1: Create CodeTabs.astro**

All snippets are real API taken from the quickstart and guides — if the API changes, these change with the guides.

```astro
---
// Tabbed API showcase. Static in phase 1; phase 3 swaps internals to make
// these runnable without touching index.mdx.
import { Tabs, TabItem, Code } from '@astrojs/starlight/components';

const fill = `import { PdfDocument } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.load(bytes);
const form = doc.getForm();

form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA, IGNACIO");
form.getCheckBox("declaracion.acepta").check();
form.flattenField("beneficiario.apellidos_nombres");

const output = await doc.save(); // incremental, append-only`;

const generate = `import { PdfDocument, PageSizes, StandardFonts, rgb } from "@ignaciano3/better-pdf";

const doc = await PdfDocument.create();
const page = doc.addPage(PageSizes.A4);

page.drawText("Hello, world!", {
  x: 180, y: 750, size: 24,
  font: doc.getFont(StandardFonts.Helvetica),
  color: rgb(0.1, 0.2, 0.8),
});

const output = await doc.save();`;

const typed = `// bunx better-pdf-generate-types form.pdf src/form-types.ts
import { myFormFields } from "./form-types.js";

const form = doc.getForm<typeof myFormFields>();
form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
form.getDropdown("beneficiario.estado_civil").select("Casado");
// Unknown names, wrong types, invalid options: compile errors.`;

const sign = `const signature = new Uint8Array(await Bun.file("signature.png").arrayBuffer());

form.getSignature("firma.titular").setImage(signature);
// Visual appearance only — not a cryptographic signature.

const output = await doc.save();`;
---

<section class="code-tabs">
	<h2 id="the-api">The API is the pitch</h2>
	<Tabs>
		<TabItem label="Fill & flatten"><Code code={fill} lang="ts" title="fill.ts" /></TabItem>
		<TabItem label="Generate"><Code code={generate} lang="ts" title="generate.ts" /></TabItem>
		<TabItem label="Typed forms"><Code code={typed} lang="ts" title="typed.ts" /></TabItem>
		<TabItem label="Sign"><Code code={sign} lang="ts" title="sign.ts" /></TabItem>
	</Tabs>
</section>

<style>
	.code-tabs { margin: 3rem 0; }
</style>
```

- [ ] **Step 2: Build**

Run: `bun run build` → exits 0.

- [ ] **Step 3: Commit**

```bash
git add docs/site/src/components/landing/CodeTabs.astro
git commit -m "docs(site): landing CodeTabs component with real API snippets"
```

---

### Task 6: EditorPromo, FooterCta & InternalsTeaser components

**Files:**
- Create: `docs/site/src/assets/editor-og.png` (downloaded)
- Create: `docs/site/src/components/landing/EditorPromo.astro`
- Create: `docs/site/src/components/landing/FooterCta.astro`
- Create: `docs/site/src/components/landing/InternalsTeaser.astro`

**Interfaces:**
- Consumes: `--bp-*` tokens; `astro:assets` `Image`.
- Produces: `<EditorPromo />`, `<FooterCta />` (used in Task 7). `<InternalsTeaser />` is built but NOT rendered until phase 2 ships (spec §Landing 6).

- [ ] **Step 1: Download the editor's OG image as the showcase asset**

```bash
curl -sL -o docs/site/src/assets/editor-og.png https://better-pdf.netlify.app/og.png
file docs/site/src/assets/editor-og.png   # expect: PNG image data
```

- [ ] **Step 2: Create EditorPromo.astro**

```astro
---
// Cross-promo band for the commercial editor built on this library.
// Doubles as social proof: the library powers a real product.
import { Image } from 'astro:assets';
import editorShot from '../../assets/editor-og.png';
---

<section class="promo">
	<div class="text">
		<h2 id="editor">Not a developer?</h2>
		<p>
			<strong>Better PDF Web</strong> — fill forms, sign, merge, and split PDFs
			in your browser. Built on this library. Free, private, no watermarks.
		</p>
		<a class="btn" href="https://better-pdf.netlify.app/" rel="noopener">Open the editor</a>
	</div>
	<Image src={editorShot} alt="Better PDF Web editor" class="shot" />
</section>

<style>
	.promo {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2rem;
		align-items: center;
		background: var(--bp-surface);
		border: 1px solid var(--bp-border);
		border-radius: 0.5rem;
		padding: 2rem;
		margin: 3rem 0;
	}
	@media (max-width: 40rem) {
		.promo { grid-template-columns: 1fr; }
	}
	.text p { color: var(--sl-color-gray-2); }
	.btn {
		display: inline-block;
		background: var(--bp-accent-fill);
		color: #ffffff;
		font-weight: 600;
		padding: 0.55rem 1.2rem;
		border-radius: 0.375rem;
		text-decoration: none;
		margin-top: 0.75rem;
	}
	.btn:hover { background: #1d4ed8; }
	.shot { border-radius: 0.375rem; border: 1px solid var(--bp-border); height: auto; }
</style>
```

- [ ] **Step 3: Create FooterCta.astro**

```astro
---
// Final CTA band: repeat install + entry link, then resource links.
---

<section class="footer-cta">
	<code>$ npm i @ignaciano3/better-pdf</code>
	<a class="btn" href="/better-pdf/getting-started/introduction/">Get started</a>
	<nav aria-label="Project links">
		<a href="https://github.com/ignaciano3/better-pdf" rel="noopener">GitHub</a>
		<a href="https://www.npmjs.com/package/@ignaciano3/better-pdf" rel="noopener">npm</a>
		<a href="/better-pdf/reference/changelog/">Changelog</a>
		<a href="https://better-pdf.netlify.app/" rel="noopener">Editor</a>
		<a href="https://github.com/ignaciano3/better-pdf/blob/master/LICENSE" rel="noopener">License</a>
	</nav>
</section>

<style>
	.footer-cta {
		text-align: center;
		margin: 4rem 0 2rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1rem;
	}
	code {
		font-family: var(--sl-font-mono);
		background: var(--bp-surface);
		border: 1px solid var(--bp-border);
		color: var(--bp-accent-text);
		padding: 0.5rem 1rem;
		border-radius: 0.375rem;
	}
	.btn {
		background: var(--bp-accent-fill);
		color: #ffffff;
		font-weight: 600;
		padding: 0.6rem 1.4rem;
		border-radius: 0.375rem;
		text-decoration: none;
	}
	.btn:hover { background: #1d4ed8; }
	nav { display: flex; gap: 1.25rem; }
	nav a { color: var(--sl-color-gray-3); }
</style>
```

- [ ] **Step 4: Create InternalsTeaser.astro (built, not rendered — phase-2 gate)**

```astro
---
// "How PDFs work" teaser. NOT rendered on the landing page until the
// phase-2 internals section exists — see the phase-1 spec (§Landing, item 6).
---

<section class="teaser">
	<pre aria-hidden="true">xref
0 21
0000000000 65535 f
0000000017 00000 n</pre>
	<div>
		<h2 id="internals">How PDFs actually work</h2>
		<p>Objects, xref tables, incremental updates — written down properly.</p>
		<a href="/better-pdf/internals/">Read the internals section</a>
	</div>
</section>

<style>
	.teaser {
		display: flex;
		gap: 2rem;
		align-items: center;
		border: 1px solid var(--bp-border);
		border-radius: 0.5rem;
		padding: 1.5rem 2rem;
		margin: 3rem 0;
	}
	pre {
		color: var(--bp-texture);
		font-size: 0.8rem;
		user-select: none;
		background: none;
		border: none;
	}
	.teaser a { color: var(--bp-accent-text); }
</style>
```

- [ ] **Step 5: Build**

Run: `bun run build` → exits 0.

- [ ] **Step 6: Commit**

```bash
git add docs/site/src/assets/editor-og.png docs/site/src/components/landing/EditorPromo.astro docs/site/src/components/landing/FooterCta.astro docs/site/src/components/landing/InternalsTeaser.astro
git commit -m "docs(site): EditorPromo, FooterCta, and gated InternalsTeaser components"
```

---

### Task 7: Landing assembly & final verification

**Files:**
- Rewrite: `docs/site/src/content/docs/index.mdx`

**Interfaces:**
- Consumes: all six components from Tasks 3–6 (InternalsTeaser imported nowhere — gated).

- [ ] **Step 1: Rewrite index.mdx**

Replace the entire file (drops the frontmatter `hero:` block and the `CardGrid`):

```mdx
---
title: better-pdf
description: A maintained, fast alternative to pdf-lib for PDF AcroForms and document generation.
template: splash
---

import Hero from '../../components/landing/Hero.astro';
import ProofBar from '../../components/landing/ProofBar.astro';
import CodeTabs from '../../components/landing/CodeTabs.astro';
import CompareTable from '../../components/landing/CompareTable.astro';
import EditorPromo from '../../components/landing/EditorPromo.astro';
import FooterCta from '../../components/landing/FooterCta.astro';

<Hero />
<ProofBar />
<CodeTabs />
<CompareTable />
<EditorPromo />
<FooterCta />
```

- [ ] **Step 2: Build**

Run: `bun run build` → exits 0.

- [ ] **Step 3: Full manual verification (spec §Verification)**

In `bun run dev`:
1. Landing renders all six sections in order, both themes (toggle), no horizontal scroll at 375 px width (responsive check).
2. Copy button copies `npm i @ignaciano3/better-pdf` and announces via the status region (inspect with VoiceOver or check `[data-copy-status]` text change).
3. One guide page + one generated API page still styled correctly.
4. Stats identical to `reference/benchmarks.md`.

- [ ] **Step 4: Lighthouse accessibility ≥ 95**

```bash
cd docs/site && bun run build && bun run preview &   # serves dist at localhost:4321
CHROME_PATH="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser" \
  npx lighthouse http://localhost:4321/better-pdf/ --only-categories=accessibility --chrome-flags="--headless" --output=json --output-path=/tmp/lh.json
python3 -c "import json; print(json.load(open('/tmp/lh.json'))['categories']['accessibility']['score'])"
```

Expected: score ≥ 0.95. If below: the report's `audits` section lists the failing pairs — fix and re-run.

- [ ] **Step 5: Commit**

```bash
git add docs/site/src/content/docs/index.mdx
git commit -m "docs(site): rebuild landing page with seven-section pitch"
```
