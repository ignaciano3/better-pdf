import { expect, test } from "bun:test";
import type { FieldInfo } from "../src/form.ts";
import { generateFormTypes } from "../src/typegen.ts";

const fields: FieldInfo[] = [
  {
    name: "beneficiario.apellidos_nombres",
    type: "text",
    value: null,
    states: [],
    options: [],
    readOnly: false,
  },
  {
    name: "beneficiario.estado_civil",
    type: "dropdown",
    value: "Soltero",
    states: [],
    options: ["Soltero", "Casado"],
    readOnly: false,
  },
  {
    name: "beneficiario.tipo_beneficiario",
    type: "radio",
    value: "Titular",
    states: ["Titular", "Familiar"],
    options: [],
    readOnly: false,
  },
];

test("generates field name and typed metadata declarations", () => {
  const source = generateFormTypes(fields, { typeName: "AnexoForm" });

  expect(source).toContain("export const anexoFormFields = {");
  expect(source).toContain(
    'export type AnexoFormFieldName = "beneficiario.apellidos_nombres" | "beneficiario.estado_civil" | "beneficiario.tipo_beneficiario";',
  );
  expect(source).toContain('export type AnexoFormTextFieldName = AnexoFormFieldNameByType<"text">;');
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
