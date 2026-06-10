import type { FieldInfo, FieldType } from "./form.js";
import type {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfSignature,
} from "./fields.js";

/** The compile-time shape of one generated field's metadata entry. */
export interface FieldMeta {
  type: FieldType;
  readOnly: boolean;
  value: string | null;
  states: readonly string[];
  options: readonly string[];
}

/** The shape of a generated `…Fields` metadata object (i.e. `typeof myFormFields`). */
export type FormSchema = Record<string, FieldMeta>;

/** Every field name declared in a schema. */
export type FieldNameOf<S extends FormSchema> = Extract<keyof S, string>;

/** The names in a schema whose field type is exactly `K`. */
export type NameOfType<S extends FormSchema, K extends FieldType> = {
  [N in keyof S]: S[N]["type"] extends K ? N : never;
}[keyof S] &
  string;

/** Valid values for a choice field: its options (dropdown) or its on-states (radio). */
export type OptionsOf<S extends FormSchema, N extends keyof S> =
  | S[N]["options"][number]
  | S[N]["states"][number];

/**
 * A compile-time-narrowed view over a `PdfForm`, produced by
 * `doc.getForm<typeof myFormFields>()`. This is purely a type overlay: the
 * runtime object is the same untyped `PdfForm`.
 */
export interface TypedPdfForm<S extends FormSchema> {
  getFields(): FieldInfo[];
  getField(name: FieldNameOf<S>): FieldInfo | undefined;
  getTextField(name: NameOfType<S, "text">): PdfTextField;
  getCheckBox(name: NameOfType<S, "checkbox">): PdfCheckBox;
  getRadioGroup<N extends NameOfType<S, "radio">>(name: N): PdfRadioGroup<OptionsOf<S, N>>;
  getDropdown<N extends NameOfType<S, "dropdown">>(name: N): PdfDropdown<OptionsOf<S, N>>;
  getSignature(name: NameOfType<S, "signature">): PdfSignature;
  flattenField(name: FieldNameOf<S>): void;
  flatten(): void;
}
