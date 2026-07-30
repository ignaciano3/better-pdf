import type { FieldInfo, FieldType } from "./form.js";
import type {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./fields.js";

/**
 * The compile-time shape of one declared field's metadata, generic over the
 * field `type`, its radio `states`, and its choice `options`. {@link FieldMeta}
 * is the widened form; the {@link FormBuilder} threads concrete instantiations
 * through its `addX` methods. Keeping this in one place stops the builder's
 * accumulated schema from drifting from `FieldMeta`.
 */
export type DeclaredField<
  T extends FieldType,
  St extends readonly string[],
  Opt extends readonly string[],
> = {
  type: T;
  readOnly: boolean;
  value: string | null;
  states: St;
  options: Opt;
  multiSelect: boolean;
};

/** The compile-time shape of one generated field's metadata entry. */
export type FieldMeta = DeclaredField<FieldType, readonly string[], readonly string[]>;

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
  /** Get metadata for every AcroForm field in the document. */
  getFields(): FieldInfo[];
  /** Get metadata for one declared field name, or `undefined` if it is absent. */
  getField(name: FieldNameOf<S>): FieldInfo | undefined;
  /** Get a typed text field wrapper. */
  getTextField(name: NameOfType<S, "text">): PdfTextField;
  /** Get a typed checkbox wrapper. */
  getCheckBox(name: NameOfType<S, "checkbox">): PdfCheckBox;
  /** Get a typed radio group wrapper whose `select()` values come from the schema. */
  getRadioGroup<N extends NameOfType<S, "radio">>(name: N): PdfRadioGroup<OptionsOf<S, N>>;
  /** Get a typed dropdown wrapper whose `select()` values come from the schema. */
  getDropdown<N extends NameOfType<S, "dropdown">>(name: N): PdfDropdown<OptionsOf<S, N>>;
  /**
   * Get a typed list-box wrapper whose `select()` values come from the schema.
   * For multi-select list boxes, use the runtime-guarded `selectMultiple()` method.
   */
  getListBox<N extends NameOfType<S, "listbox">>(name: N): PdfListBox<OptionsOf<S, N>>;
  /** Get a typed visual signature field wrapper. */
  getSignature(name: NameOfType<S, "signature">): PdfSignature;
  /** Queue one declared field to be flattened when the document is saved. */
  flattenField(name: FieldNameOf<S>): void;
  /** Queue every field to be flattened when the document is saved. */
  flatten(): void;
  /** Queue one declared field to be reset to its default value on save. */
  resetField(name: FieldNameOf<S>): void;
  /** Queue every value-bearing field to be reset to its default value on save. */
  reset(): void;
}
