import {
  FillQueue,
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./fields.js";
import { UnknownFieldError, FieldTypeError } from "../core/errors.js";
import { kFormQueue, kFlattenQueue } from "../core/internal.js";

export type FieldType =
  | "text" | "checkbox" | "radio" | "dropdown"
  | "listbox" | "signature" | "pushbutton" | "unknown";

export interface FieldWidget {
  /** 0-based page index the widget is on. */
  page: number;
  /** `/Rect` `[x0, y0, x1, y1]` in PDF points (origin bottom-left). */
  rect: [number, number, number, number];
  /** Annotation `/F` Hidden flag (bit 2): not displayed and not printed. */
  hidden: boolean;
  /** Annotation `/F` Print flag (bit 3): included when the page is printed. */
  print: boolean;
  /** Annotation `/F` NoView flag (bit 6): hidden on screen but may still print. */
  noView: boolean;
}

export interface FieldInfo {
  /** Fully-qualified field name (ancestor /T joined by "."). */
  name: string;
  /** The field type: `"text" | "checkbox" | "radio" | "dropdown" |
   * "listbox" | "signature" | "pushbutton" | "unknown"`. */
  type: FieldType;
  /** Current value as a string, or null if unset. */
  value: string | null;
  /** The field's default/reset value (`/DV`), or null if it has none. */
  defaultValue: string | null;
  /** On-state export values for checkbox/radio; empty otherwise. */
  states: string[];
  /** Option export values for dropdown/listbox; empty otherwise. */
  options: string[];
  /** True if the field has the ReadOnly flag set (`/Ff` bit 1). */
  readOnly: boolean;
  /** True if the field has the Required flag set. */
  required: boolean;
  /** False if the field has the NoExport flag set; true otherwise. */
  exported: boolean;
  /** Text field `/MaxLen`, or null for non-text fields / when undeclared. */
  maxLength: number | null;
  /** True only for multi-select list boxes (the PDF Multiselect choice flag). */
  multiSelect: boolean;
  /** True only for password text fields (the PDF Password text flag): the value
   * should be masked rather than displayed. */
  password: boolean;
  /** True only for multi-line text fields (the PDF Multiline text flag). */
  multiline: boolean;
  /** True only for comb text fields (the PDF Comb text flag): a single line
   * split into `maxLength` fixed-pitch per-character cells. */
  comb: boolean;
  /** True only for editable dropdowns (the combo box Edit flag): the user may
   * type a value that is not one of `options`. */
  editable: boolean;
  /** Horizontal alignment of the field's text, from `/Q`. Defaults to `"left"`
   * when the field declares none. */
  align: "left" | "center" | "right";
  /** The field's tooltip / alternate descriptive name (`/TU`), or null when the
   * field has none. */
  tooltip: string | null;
  /** For variable-text fields (text/dropdown/listbox), the font resource name
   * from the effective `/DA` (e.g. `"Helv"`); null for other field types or when
   * no `/DA` applies. */
  fontName: string | null;
  /** For variable-text fields, the font size in points from the effective
   * `/DA`. `0` means auto-size (the PDF `0 Tf` convention); null for other field
   * types or when no `/DA` applies. */
  fontSize: number | null;
  /** One entry per widget annotation (page + position). Usually one; radio
   * groups and fields repeated across pages have several. */
  widgets: FieldWidget[];
}

export type ReadFields = (bytes: Uint8Array) => string;

/**
 * Provides access to the AcroForm fields in a PDF.
 *
 * Use `PdfForm` to inspect fields, get type-specific field wrappers, queue
 * field values, and choose which fields should be flattened when the document
 * is saved.
 *
 * @example
 * ```ts
 * const form = doc.getForm();
 *
 * form.getTextField("applicant.name").setText("Ada Lovelace");
 * form.getDropdown("applicant.country").select("Argentina");
 * form.getCheckBox("applicant.acceptsTerms").check();
 *
 * const pdfBytes = await doc.save();
 * ```
 */
export class PdfForm {
  private readonly fields: FieldInfo[];
  /** @internal — shared with PdfDocument so save() can flush pending ops. */
  readonly [kFormQueue] = new FillQueue();
  /**
   * @internal — fully-qualified names queued for flattening on save. A Set
   * gives O(1) dedupe; insertion order (which the save pipeline preserves)
   * matches the old array.
   */
  readonly [kFlattenQueue] = new Set<string>();

  /** @internal */
  constructor(bytes: Uint8Array, readFields: ReadFields) {
    this.fields = JSON.parse(readFields(bytes)) as FieldInfo[];
  }

  /**
   * Get metadata for every AcroForm field in the document.
   *
   * The returned field info includes each field's fully-qualified name, type,
   * current value, valid checkbox/radio states, valid choice options, flags,
   * max text length, and widget positions.
   *
   * @returns All fields in the form.
   *
   * @example
   * ```ts
   * for (const field of form.getFields()) {
   *   console.log(field.name, field.type, field.value);
   *   console.log(field.options);
   * }
   * ```
   */
  getFields(): FieldInfo[] {
    return this.fields;
  }

  /**
   * Get metadata for one field by fully-qualified field name.
   *
   * This returns `undefined` when the field does not exist. Use a type-specific
   * accessor such as `getTextField()` when you want an exception for missing
   * fields or wrong field types.
   *
   * @param name - The fully-qualified field name.
   * @returns The field metadata, or `undefined` if no field has that name.
   *
   * @example
   * ```ts
   * const field = form.getField("beneficiario.estado_civil");
   * if (field?.type === "dropdown") {
   *   console.log(field.options);
   * }
   * ```
   */
  getField(name: string): FieldInfo | undefined {
    return this.fields.find((f) => f.name === name);
  }

  private require(name: string, type: FieldType): FieldInfo {
    const f = this.getField(name);
    if (!f) throw new UnknownFieldError(name);
    if (f.type !== type) {
      throw new FieldTypeError(name, f.type, type);
    }
    return f;
  }

  /**
   * Get a text field by name.
   *
   * @param name - The fully-qualified text field name.
   * @returns A text field wrapper for setting the field value.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a text field.
   *
   * @example
   * ```ts
   * form.getTextField("person.fullName").setText("Ada Lovelace");
   * ```
   */
  getTextField(name: string): PdfTextField {
    return new PdfTextField(this.require(name, "text"), this[kFormQueue]);
  }
  /**
   * Get a checkbox field by name.
   *
   * @param name - The fully-qualified checkbox field name.
   * @returns A checkbox wrapper for checking or unchecking the field.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a checkbox.
   *
   * @example
   * ```ts
   * form.getCheckBox("person.accepted").check();
   * ```
   */
  getCheckBox(name: string): PdfCheckBox {
    return new PdfCheckBox(this.require(name, "checkbox"), this[kFormQueue]);
  }
  /**
   * Get a radio-button group by name.
   *
   * Select one of the group's real export values. You can read them from
   * `radioGroup.options` or from `FieldInfo.states`.
   *
   * @param name - The fully-qualified radio group field name.
   * @returns A radio group wrapper for selecting an option.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a radio group.
   *
   * @example
   * ```ts
   * const radio = form.getRadioGroup("beneficiario.tipo_beneficiario");
   * console.log(radio.options);
   * radio.select("Titular");
   * ```
   */
  getRadioGroup(name: string): PdfRadioGroup {
    return new PdfRadioGroup(this.require(name, "radio"), this[kFormQueue]);
  }
  /**
   * Get a dropdown field by name.
   *
   * Select one of the dropdown's real export values. You can read them from
   * `dropdown.options` or from `FieldInfo.options`.
   *
   * @param name - The fully-qualified dropdown field name.
   * @returns A dropdown wrapper for selecting an option.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a dropdown.
   *
   * @example
   * ```ts
   * const dropdown = form.getDropdown("beneficiario.estado_civil");
   * dropdown.select(dropdown.options[0]);
   * ```
   */
  getDropdown(name: string): PdfDropdown {
    return new PdfDropdown(this.require(name, "dropdown"), this[kFormQueue]);
  }
  /**
   * Get a list-box field by name.
   *
   * Select one of the list box's real export values from `listBox.options` or
   * `FieldInfo.options`. For multi-select list boxes (those with the Multiselect
   * flag set), use `listBox.selectMultiple(values)` instead.
   *
   * @param name - The fully-qualified list-box field name.
   * @returns A list-box wrapper for selecting an option.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a list box.
   *
   * @example
   * ```ts
   * const listBox = form.getListBox("person.languages");
   * listBox.select("TypeScript");
   * ```
   */
  getListBox(name: string): PdfListBox {
    return new PdfListBox(this.require(name, "listbox"), this[kFormQueue]);
  }
  /**
   * Get a visual signature field by name.
   *
   * This is for placing a signature image only. It does not create a
   * cryptographic digital signature.
   *
   * @param name - The fully-qualified signature field name.
   * @returns A signature wrapper for setting a visual signature image.
   * @throws `UnknownFieldError` when no field has the given name.
   * @throws `FieldTypeError` when the field exists but is not a signature field.
   *
   * @example
   * ```ts
   * const image = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
   * form.getSignature("firma.titular").setImage(image);
   * ```
   */
  getSignature(name: string): PdfSignature {
    return new PdfSignature(this.require(name, "signature"), this[kFormQueue]);
  }

  /**
   * Queue one field to be flattened when the document is saved.
   *
   * Flattening turns the field's current appearance into normal page content
   * and removes the interactive field from the saved PDF. If you filled the
   * field first, the filled value is flattened.
   *
   * @param name - The fully-qualified field name to flatten.
   * @throws `UnknownFieldError` when no field has the given name.
   *
   * @example
   * ```ts
   * form.getTextField("invoice.total").setText("$42.00");
   * form.flattenField("invoice.total");
   *
   * const pdfBytes = await doc.save();
   * ```
   */
  flattenField(name: string): void {
    if (!this.getField(name)) throw new UnknownFieldError(name);
    this[kFlattenQueue].add(name);
  }

  /**
   * Queue every form field to be flattened when the document is saved.
   *
   * Flattening all fields is useful when you want the saved PDF to be
   * non-editable after filling it.
   *
   * @example
   * ```ts
   * form.getTextField("person.fullName").setText("Ada Lovelace");
   * form.flatten();
   *
   * const pdfBytes = await doc.save();
   * ```
   */
  flatten(): void {
    for (const f of this.fields) {
      this[kFlattenQueue].add(f.name);
    }
  }

  /**
   * Queue one field to be reset to its default value when the document is saved.
   *
   * Resetting sets the field's value to its default value (`/DV`), or clears it
   * when the field has none — the same effect as a PDF viewer's "reset form" for
   * that field. The change is written when you call `doc.save()`.
   *
   * @param name - The fully-qualified field name to reset.
   * @throws `UnknownFieldError` when no field has the given name.
   *
   * @example
   * ```ts
   * form.resetField("applicant.country");
   * const pdfBytes = await doc.save();
   * ```
   */
  resetField(name: string): void {
    const f = this.getField(name);
    if (!f) throw new UnknownFieldError(name);
    this[kFormQueue].push({ name, reset: true });
    f.value = f.defaultValue;
  }

  /**
   * Queue every value-bearing field to be reset to its default value when the
   * document is saved.
   *
   * This is the equivalent of a PDF viewer's "reset form": each text, checkbox,
   * radio, dropdown, and list-box field is set to its default value (`/DV`), or
   * cleared when it has none. Signature and push-button fields are skipped.
   *
   * @example
   * ```ts
   * form.reset();
   * const pdfBytes = await doc.save();
   * ```
   */
  reset(): void {
    for (const f of this.fields) {
      if (RESETTABLE_TYPES.has(f.type)) {
        this[kFormQueue].push({ name: f.name, reset: true });
        f.value = f.defaultValue;
      }
    }
  }
}

/** Field types that carry a value and can therefore be reset to their `/DV`. */
const RESETTABLE_TYPES: ReadonlySet<FieldType> = new Set<FieldType>([
  "text",
  "checkbox",
  "radio",
  "dropdown",
  "listbox",
]);

export { kFormQueue, kFlattenQueue };
