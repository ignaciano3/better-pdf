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
    states: [],
    options: [],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: 40,
    multiSelect: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
    widgets: [],
  },
  {
    name: "beneficiario.estado_civil",
    type: "dropdown",
    value: "Soltero",
    states: [],
    options: ["Soltero", "Casado"],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: null,
    multiSelect: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
    widgets: [],
  },
  {
    name: "beneficiario.tipo_beneficiario",
    type: "radio",
    value: "Titular",
    states: ["Titular", "Familiar"],
    options: [],
    readOnly: false,
    required: false,
    exported: true,
    maxLength: null,
    multiSelect: false,
    multiline: false,
    comb: false,
    editable: false,
    align: "left",
    tooltip: null,
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
  expect(source).toContain("required: false,");
  expect(source).toContain("exported: true,");
  expect(source).toContain("maxLength: 40,");
  expect(source).toContain("maxLength: null,");
  expect(source).toContain("multiSelect: false,");
  expect(source).toContain('options: ["Soltero", "Casado"] as const');
  expect(source).toContain('states: ["Titular", "Familiar"] as const');
  expect(source).toContain(
    "export type AnexoFormOptions<TName extends AnexoFormChoiceFieldName>",
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
