// @ts-check
import { defineConfig } from 'astro/config';
import { fileURLToPath } from 'node:url';
import { readFileSync, writeFileSync } from 'node:fs';
import starlight from '@astrojs/starlight';
import { createStarlightTypeDocPlugin } from 'starlight-typedoc';

const [starlightTypeDoc, typeDocSidebarGroup] = createStarlightTypeDocPlugin();

/**
 * Mirror the repo-root CHANGELOG.md into a Starlight reference page so the
 * published docs always match the released changelog. Runs on every `dev` and
 * `build`, so there's nothing to remember to update — edit CHANGELOG.md only.
 * The generated page is gitignored.
 * @returns {import('astro').AstroIntegration}
 */
function syncChangelog() {
	return {
		name: 'sync-changelog',
		hooks: {
			'astro:config:setup'({ logger }) {
				const source = new URL('../../CHANGELOG.md', import.meta.url);
				const dest = new URL(
					'./src/content/docs/reference/changelog.md',
					import.meta.url,
				);
				const raw = readFileSync(fileURLToPath(source), 'utf8');
				// Drop the leading `# Changelog` H1 — Starlight renders the title
				// from frontmatter instead.
				const body = raw.replace(/^#\s+Changelog\s*\n+/, '');
				const page = `---
title: Changelog
description: Release history for better-pdf. Generated from CHANGELOG.md — do not edit.
editUrl: false
---

<!-- This page is generated from the repo-root CHANGELOG.md by astro.config.mjs. -->

${body}`;
				writeFileSync(fileURLToPath(dest), page);
				logger.info('Synced CHANGELOG.md → reference/changelog.md');
			},
		},
	};
}

// https://astro.build/config
export default defineConfig({
	// GitHub Pages: https://ignaciano3.github.io/better-pdf
	site: 'https://ignaciano3.github.io',
	base: '/better-pdf',
	integrations: [
		syncChangelog(),
		starlight({
			title: 'better-pdf',
			description:
				'A maintained, fast alternative to pdf-lib for filling and flattening PDF AcroForms and generating documents.',
			logo: {
				src: './src/assets/logo.svg',
				alt: 'better-pdf',
			},
			components: {
				PageTitle: './src/components/PageTitle.astro',
			},
			customCss: [
				'@fontsource/ibm-plex-sans/400.css',
				'@fontsource/ibm-plex-sans/500.css',
				'@fontsource/ibm-plex-sans/600.css',
				'@fontsource/ibm-plex-mono/400.css',
				'@fontsource/ibm-plex-mono/500.css',
				'@fontsource/ibm-plex-mono/600.css',
				'./src/styles/custom.css',
			],
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
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/ignaciano3/better-pdf',
				},
			],
			editLink: {
				baseUrl:
					'https://github.com/ignaciano3/better-pdf/edit/master/docs/site/',
			},
			plugins: [
				starlightTypeDoc({
					entryPoints: ['../../src/index.ts'],
					tsconfig: '../../tsconfig.json',
					output: 'api-reference',
					sidebar: { label: 'API (generated)', collapsed: true },
					typeDoc: {
						excludeInternal: true,
						excludePrivate: true,
						hideGenerator: true,
						sort: ['source-order'],
					},
				}),
			],
			sidebar: [
				{
					label: 'Getting started',
					items: [
						{ label: 'Introduction', slug: 'getting-started/introduction' },
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quickstart', slug: 'getting-started/quickstart' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Filling & flattening forms', slug: 'guides/filling-forms' },
						{ label: 'Signatures', slug: 'guides/signatures' },
						{ label: 'Generating & drawing', slug: 'guides/generating' },
						{ label: 'Creating form fields', slug: 'guides/creating-form-fields' },
						{ label: 'Typed forms', slug: 'guides/typed-forms' },
						{ label: 'Browser usage', slug: 'guides/browser' },
						{ label: 'Runtime setup', slug: 'guides/runtimes' },
						{ label: 'For AI agents', slug: 'guides/ai-agents' },
					],
				},
				{
					label: 'Examples',
					items: [
						{ label: 'Overview', slug: 'examples/overview' },
						{ label: 'Fill & flatten a form', slug: 'examples/fill-and-flatten' },
						{ label: 'Generate an invoice', slug: 'examples/invoice' },
						{ label: 'Merge PDFs', slug: 'examples/merge-pdfs' },
					],
				},
				{
					label: 'How PDFs work',
					items: [
						{ label: 'Overview', slug: 'internals' },
						{ label: 'File anatomy', slug: 'internals/file-anatomy' },
						{ label: 'Objects & xref', slug: 'internals/objects-and-xref' },
						{ label: 'Incremental updates', slug: 'internals/incremental-updates' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'API', slug: 'reference/api' },
						{ label: 'Errors', slug: 'reference/errors' },
						{ label: 'Limitations', slug: 'reference/limitations' },
						{ label: 'Benchmarks', slug: 'reference/benchmarks' },
						{ label: 'Changelog', slug: 'reference/changelog' },
					],
				},
				typeDocSidebarGroup,
				{
					label: 'Migrating',
					items: [{ label: 'From pdf-lib', slug: 'migrating/from-pdf-lib' }],
				},
			],
		}),
	],
});
