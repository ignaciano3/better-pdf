import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { FieldInfo } from "../src/forms/form.ts";
import { generateFormTypes } from "../src/forms/typegen.ts";

test("typegen entry stays runtime-dependency-free (tree-shakeable)", () => {
  // The `better-pdf/typegen` subpath must not pull in the WASM core. Guard it by
  // requiring every import in the module to be type-only (erased at build).
  const src = readFileSync(join(import.meta.dir, "../src/forms/typegen.ts"), "utf8");
  const imports = src.match(/^\s*import\b.*$/gm) ?? [];
  const runtimeImports = imports.filter((line) => !/^\s*import\s+type\b/.test(line));
  expect(runtimeImports).toEqual([]);
});

const fields: FieldInfo[] = [
  {
    name: "beneficiario.apellidos_nombres",
    type: "text",
    value: null,
    defaultValue: "SIN DATO",
    states: [],
    options: [],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: 40,
    multiSelect: false,
    password: false,
    multiline: true,
    comb: false,
    editable: false,
    align: "center",
    tooltip: "Apellidos y nombres",
    fontName: "Helv",
    fontSize: 0,
    widgets: [
      { page: 0, rect: [0, 0, 10, 10], hidden: false, print: true, noView: false },
      { page: 2, rect: [0, 0, 10, 10], hidden: false, print: true, noView: false },
      { page: 2, rect: [20, 0, 30, 10], hidden: false, print: true, noView: false },
    ],
  },
  {
    name: "beneficiario.estado_civil",
    type: "dropdown",
    value: "Soltero",
    defaultValue: null,
    states: [],
    options: ["Soltero", "Casado"],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: null,
    multiSelect: false,
    password: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
    fontName: null,
    fontSize: null,
    widgets: [],
  },
  {
    name: "beneficiario.tipo_beneficiario",
    type: "radio",
    value: "Titular",
    defaultValue: null,
    states: ["Titular", "Familiar"],
    options: [],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: null,
    multiSelect: false,
    password: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
    fontName: null,
    fontSize: null,
    widgets: [],
  },
];

test("generates field name and typed metadata declarations", () => {
  const source = generateFormTypes(fields, { typeName: "AnexoForm" });

  expect(source).toContain("export const anexoFormFields = {");
  expect(source).toContain("/* Usage: const form = doc.getForm<typeof anexoFormFields>(); */");
  expect(source).toContain(
    'export type AnexoFormFieldName = "beneficiario.apellidos_nombres" | "beneficiario.estado_civil" | "beneficiario.tipo_beneficiario";',
  );
  expect(source).toContain('export type AnexoFormTextFieldName = AnexoFormFieldNameByType<"text">;');
  expect(source).toContain("multiSelect: false,");
  expect(source).toContain('options: ["Soltero", "Casado"] as const');
  expect(source).toContain('states: ["Titular", "Familiar"] as const');
  expect(source).toContain(
    "export type AnexoFormOptions<TName extends AnexoFormChoiceFieldName>",
  );
});

test("emits the descriptive schema but no field answers", () => {
  // The generated module doubles as a standalone description of the form:
  // everything that describes a field's *shape* is emitted, so it is readable
  // without loading the PDF or using this library. Field *answers* are omitted
  // so generating from a filled form never bakes data (potentially PII) into
  // source control, and regeneration never churns on values.
  const source = generateFormTypes(fields, { typeName: "AnexoForm" });

  for (const marker of [
    "type:",
    "readOnly:",
    "required:",
    "exported:",
    "maxLength:",
    "multiSelect:",
    "password:",
    "multiline:",
    "comb:",
    "editable:",
    "align:",
    "tooltip:",
    "fontName:",
    "fontSize:",
    "pages:",
    "states:",
    "options:",
  ]) {
    expect(source).toContain(marker);
  }

  expect(source).toContain("defaultValue:");
  expect(source).not.toContain("value:");
});

test("includeValues opts the current values back in", () => {
  const source = generateFormTypes(fields, { typeName: "AnexoForm", includeValues: true });

  expect(source).toContain("value:");
  expect(source).toContain("defaultValue:");
});

test("per-field block matches the emitted shape byte-for-byte", () => {
  const source = generateFormTypes(fields, { typeName: "AnexoForm" });
  expect(source).toContain(
    `"beneficiario.apellidos_nombres": {
    type: "text",
    readOnly: false,
    required: false,
    exported: true,
    maxLength: 40,
    multiSelect: false,
    password: false,
    multiline: true,
    comb: false,
    editable: false,
    align: "center",
    tooltip: "Apellidos y nombres",
    fontName: "Helv",
    fontSize: 0,
    defaultValue: "SIN DATO",
    pages: [0, 2] as const,
    states: [] as const,
    options: [] as const,
  },`,
  );
});

test("FieldType covers every field type, not only the ones present", () => {
  // Regression: the per-type name aliases (e.g. AnexoFormCheckBoxFieldName) use
  // FieldType as their constraint. If FieldType were narrowed to the types this
  // form happens to contain, aliases for absent types would fail to compile.
  const source = generateFormTypes(fields, { typeName: "AnexoForm" });

  expect(source).toContain(
    'export type AnexoFormFieldType = "text" | "checkbox" | "radio" | "dropdown" | "listbox" | "signature" | "pushbutton" | "unknown";',
  );
  expect(source).toContain('export type AnexoFormCheckBoxFieldName = AnexoFormFieldNameByType<"checkbox">;');
});

test("rejects invalid generated type names", () => {
  expect(() => generateFormTypes(fields, { typeName: "bad-name" })).toThrow(
    /valid TypeScript identifier/,
  );
});
