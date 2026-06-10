# Milestone 11 - Form Type Generator

**Goal:** Add a first generated-types tool so users can create a TypeScript module from an existing PDF's AcroForm metadata.

**Scope for this slice:**

- Generate field-name unions from `FieldInfo[]`.
- Generate per-field metadata with literal field types, options, states, read-only flags, and current values.
- Add a CLI that loads a PDF and writes the generated TypeScript file.
- Expose the pure generator for advanced/manual usage.
- Verify the generator with tests and package build checks.

**Deferred:**

- Typed wrappers that make `doc.getForm()` itself generic.
- Optimistic typed fill helpers.
- Watch mode and multi-PDF project configs.

## Tasks

- [x] Add the pure form type generator.
- [x] Add CLI entrypoint and package bin.
- [x] Export/document the generator.
- [x] Add generator tests.
- [x] Verify tests, type-check, build, and package dry-run.
