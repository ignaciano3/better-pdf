// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { createStarlightTypeDocPlugin } from 'starlight-typedoc';

const [starlightTypeDoc, typeDocSidebarGroup] = createStarlightTypeDocPlugin();

// https://astro.build/config
export default defineConfig({
	// GitHub Pages: https://ignaciano3.github.io/better-pdf
	site: 'https://ignaciano3.github.io',
	base: '/better-pdf',
	integrations: [
		starlight({
			title: 'better-pdf',
			description:
				'A maintained, fast alternative to pdf-lib for filling and flattening PDF AcroForms and generating documents.',
			logo: {
				src: './src/assets/logo.svg',
				alt: 'better-pdf',
			},
			customCss: ['./src/styles/custom.css'],
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
						{ label: 'For AI agents', slug: 'guides/ai-agents' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'API', slug: 'reference/api' },
						{ label: 'Errors', slug: 'reference/errors' },
						{ label: 'Limitations', slug: 'reference/limitations' },
						{ label: 'Benchmarks', slug: 'reference/benchmarks' },
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
