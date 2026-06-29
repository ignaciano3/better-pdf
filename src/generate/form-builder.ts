import type { Color } from "./color.js";
import type { FieldNameOf, FormSchema } from "../forms/schema.js";
import { StandardFonts } from "./fonts.js";

// ---------------------------------------------------------------------------
// Public option interfaces
// ---------------------------------------------------------------------------

export interface FieldBorder {
  color: Color;
  width?: number;
}

interface BaseFieldOptions {
  page: number;
  x: number;
  y: number;
  required?: boolean;
  readOnly?: boolean;
  tooltip?: string;
  border?: FieldBorder;
  background?: Color;
  /** Color of the field's text/value. Defaults to black. */
  textColor?: Color;
}

/** Horizontal alignment of a field's text/value. Defaults to `"left"`. */
export type FieldAlign = "left" | "center" | "right";

export interface TextFieldOptions extends BaseFieldOptions {
  width: number;
  height: number;
  value?: string;
  /** Default/reset value (`/DV`), independent of `value`. Restored by a viewer's
   * "reset form". Must not be longer than `maxLength`. */
  defaultValue?: string;
  maxLength?: number;
  multiline?: boolean;
  /** Mask the field's display (the PDF Password flag): viewers render the value
   * as dots/asterisks instead of the characters. Defaults to `false`. */
  password?: boolean;
  /**
   * Render as a comb field: a single line split into `maxLength` equal cells,
   * one character per cell (e.g. SSN or date boxes). Requires `maxLength` and is
   * incompatible with `multiline`.
   */
  comb?: boolean;
  /** Horizontal alignment of the field's text. Defaults to `"left"`. */
  align?: FieldAlign;
  /** Font size in points for the field's value. Defaults to 12. */
  fontSize?: number;
  /** Standard-14 font for the field's value. Defaults to Helvetica. Embedded
   * (PdfFont) fonts are not supported for form fields. */
  font?: StandardFonts;
}

/**
 * The mark drawn when a checkbox or radio button is selected. Defaults to
 * `"check"` for checkboxes and `"circle"` for radio buttons.
 */
export type CheckStyle = "check" | "cross" | "circle" | "square" | "diamond" | "star";

export interface CheckBoxOptions extends BaseFieldOptions {
  size: number;
  checked?: boolean;
  /** Default/reset state (`/DV`), independent of `checked`. Restored by a
   * viewer's "reset form". */
  defaultChecked?: boolean;
  onValue?: string;
  /** The mark drawn when ticked. Defaults to `"check"`. */
  checkStyle?: CheckStyle;
}

export interface RadioOption {
  value: string;
  page: number;
  x: number;
  y: number;
  size: number;
}

export interface RadioGroupOptions {
  selected?: string;
  /** Default/reset selection (`/DV`), independent of `selected`. Restored by a
   * viewer's "reset form". Must be one of the option values. */
  defaultSelected?: string;
  required?: boolean;
  readOnly?: boolean;
  tooltip?: string;
  options: readonly RadioOption[];
  /** The mark drawn when a button is selected. Defaults to `"circle"`. */
  checkStyle?: CheckStyle;
}

export interface ChoiceOptions<O extends string> extends BaseFieldOptions {
  width: number;
  height: number;
  options: readonly O[];
  selected?: NoInfer<O>;
  /** Default/reset selection (`/DV`), independent of `selected`. Restored by a
   * viewer's "reset form". Must be one of `options`. */
  defaultSelected?: NoInfer<O>;
  /**
   * For dropdowns ({@link FormBuilder.addDropdown}): allow the user to type a
   * custom value not in `options` (sets the combo box Edit flag). Ignored by
   * {@link FormBuilder.addListBox}, since list boxes are never combo boxes.
   */
  editable?: boolean;
  /**
   * For list boxes ({@link FormBuilder.addListBox}): allow more than one option
   * to be selected at once (sets the choice Multiselect flag). The resulting
   * field reports `FieldInfo.multiSelect === true` and accepts
   * `listBox.selectMultiple(values)`. Rejected by {@link FormBuilder.addDropdown},
   * since combo boxes are never multi-select.
   */
  multiSelect?: boolean;
  /** Horizontal alignment of the field's value. Defaults to `"left"`. */
  align?: FieldAlign;
  /** Font size in points for the field's value. Defaults to 12. */
  fontSize?: number;
  /** Standard-14 font for the field's value. Defaults to Helvetica. Embedded
   * (PdfFont) fonts are not supported for form fields. */
  font?: StandardFonts;
}

export interface SignatureFieldOptions extends BaseFieldOptions {
  width: number;
  height: number;
}

// ---------------------------------------------------------------------------
// Internal wire types (matching Rust serde tags exactly)
// ---------------------------------------------------------------------------

/** @internal */
interface WireBorder {
  color: [number, number, number];
  width: number;
}

/** @internal */
interface WireBase {
  name: string;
  page: number;
  x: number;
  y: number;
  required?: boolean;
  readOnly?: boolean;
  tooltip?: string;
  border?: WireBorder;
  background?: [number, number, number];
  textColor?: [number, number, number];
}

/** @internal */
interface WireTextField extends WireBase {
  type: "text";
  width: number;
  height: number;
  value?: string;
  defaultValue?: string;
  maxLength?: number;
  multiline?: boolean;
  password?: boolean;
  comb?: boolean;
  align?: FieldAlign;
  fontSize?: number;
  font?: string;
}

/** @internal */
interface WireCheckBox extends WireBase {
  type: "checkBox";
  size: number;
  checked?: boolean;
  defaultChecked?: boolean;
  onValue?: string;
  checkStyle?: CheckStyle;
}

/** @internal */
interface WireRadioGroup {
  type: "radioGroup";
  name: string;
  selected?: string;
  defaultSelected?: string;
  required?: boolean;
  readOnly?: boolean;
  tooltip?: string;
  options: Array<{ value: string; page: number; x: number; y: number; size: number }>;
  checkStyle?: CheckStyle;
}

/** @internal */
interface WireChoice extends WireBase {
  type: "choice";
  width: number;
  height: number;
  combo: boolean;
  editable?: boolean;
  multiselect?: boolean;
  options: string[];
  selected?: string;
  defaultSelected?: string;
  align?: FieldAlign;
  fontSize?: number;
  font?: string;
}

/** @internal */
interface WireSignature extends WireBase {
  type: "signature";
  width: number;
  height: number;
}

/** @internal */
export type FieldDef =
  | WireTextField
  | WireCheckBox
  | WireRadioGroup
  | WireChoice
  | WireSignature;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function colorToRgb(c: Color): [number, number, number] {
  return [c.red, c.green, c.blue];
}

function borderToWire(b: FieldBorder, label: string): WireBorder {
  const width = b.width ?? 1;
  assertFinite(width, `${label}.border.width`);
  return { color: colorToRgb(b.color), width };
}

function assertFinite(v: number, name: string): void {
  if (!Number.isFinite(v)) {
    throw new RangeError(`${name} must be finite, got ${v}`);
  }
}

function assertPositive(v: number, name: string): void {
  assertFinite(v, name);
  if (v <= 0) {
    throw new RangeError(`${name} must be > 0, got ${v}`);
  }
}

function assertGeometry(opts: { x: number; y: number }, label: string): void {
  assertFinite(opts.x, `${label}.x`);
  assertFinite(opts.y, `${label}.y`);
}

/**
 * Copy validated `align`/`fontSize`/`font` from a text/choice options object
 * onto its wire def. `fontSize` must be a finite number > 0. `font` must be a
 * standard-14 font name.
 */
function applyTextStyle(
  def: { align?: FieldAlign; fontSize?: number; font?: string },
  opts: { align?: FieldAlign; fontSize?: number; font?: StandardFonts },
  label: string,
): void {
  if (opts.align !== undefined) def.align = opts.align;
  if (opts.fontSize !== undefined) {
    assertPositive(opts.fontSize, `${label}.fontSize`);
    def.fontSize = opts.fontSize;
  }
  if (opts.font !== undefined) {
    if (!Object.values(StandardFonts).includes(opts.font)) {
      throw new RangeError(`${label}.font is not a standard-14 font: ${String(opts.font)}`);
    }
    def.font = opts.font;
  }
}

function buildBase(name: string, opts: BaseFieldOptions, names: Set<string>): WireBase {
  if (!name) throw new Error("Field name must be non-empty");
  if (names.has(name)) throw new Error(`Duplicate field name: "${name}"`);
  assertFinite(opts.page, `${name}.page`);
  assertGeometry(opts, name);
  const base: WireBase = { name, page: opts.page, x: opts.x, y: opts.y };
  if (opts.required !== undefined) base.required = opts.required;
  if (opts.readOnly !== undefined) base.readOnly = opts.readOnly;
  if (opts.tooltip !== undefined) base.tooltip = opts.tooltip;
  if (opts.border !== undefined) base.border = borderToWire(opts.border, name);
  if (opts.background !== undefined) base.background = colorToRgb(opts.background);
  if (opts.textColor !== undefined) base.textColor = colorToRgb(opts.textColor);
  return base;
}

// ---------------------------------------------------------------------------
// FormBuilder
// ---------------------------------------------------------------------------

/**
 * Builds an AcroForm on a document created with {@link PdfDocument.create}.
 * Obtain an instance from {@link PdfDocumentBase.createForm}.
 *
 * @typeParam S - Accumulates the declared field schema as fields are added,
 *   enabling compile-time type-checking for `getFieldNames()`.
 */
export class FormBuilder<S extends FormSchema = Record<never, never>> {
  /** @internal */
  constructor(
    private readonly defs: FieldDef[],
    private readonly names: Set<string>,
  ) {}

  // -------------------------------------------------------------------------
  // addTextField
  // -------------------------------------------------------------------------

  addTextField<N extends string>(
    name: N,
    opts: TextFieldOptions,
  ): FormBuilder<
    S & Record<N, { type: "text"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
  > {
    const base = buildBase(name, opts, this.names);
    assertPositive(opts.width, `${name}.width`);
    assertPositive(opts.height, `${name}.height`);
    if (opts.maxLength !== undefined) {
      if (!Number.isFinite(opts.maxLength) || opts.maxLength < 0) {
        throw new RangeError(`${name}.maxLength must be >= 0, got ${opts.maxLength}`);
      }
    }
    if (opts.comb) {
      if (opts.maxLength === undefined || opts.maxLength <= 0) {
        throw new RangeError(`${name}: comb field requires maxLength > 0`);
      }
      if (opts.multiline) {
        throw new RangeError(`${name}: comb field cannot be multiline`);
      }
    }
    if (
      opts.defaultValue !== undefined &&
      opts.maxLength !== undefined &&
      opts.maxLength >= 0 &&
      opts.defaultValue.length > opts.maxLength
    ) {
      throw new RangeError(
        `${name}.defaultValue length ${opts.defaultValue.length} exceeds maxLength ${opts.maxLength}`,
      );
    }
    const def: WireTextField = { ...base, type: "text", width: opts.width, height: opts.height };
    if (opts.value !== undefined) def.value = opts.value;
    if (opts.defaultValue !== undefined) def.defaultValue = opts.defaultValue;
    if (opts.maxLength !== undefined) def.maxLength = opts.maxLength;
    if (opts.multiline !== undefined) def.multiline = opts.multiline;
    if (opts.password) def.password = true;
    if (opts.comb) def.comb = true;
    applyTextStyle(def, opts, name);
    this.defs.push(def);
    this.names.add(name);
    return this as unknown as FormBuilder<
      S & Record<N, { type: "text"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
    >;
  }

  // -------------------------------------------------------------------------
  // addCheckBox
  // -------------------------------------------------------------------------

  addCheckBox<N extends string>(
    name: N,
    opts: CheckBoxOptions,
  ): FormBuilder<
    S & Record<N, { type: "checkbox"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
  > {
    const base = buildBase(name, opts, this.names);
    assertPositive(opts.size, `${name}.size`);
    const def: WireCheckBox = { ...base, type: "checkBox", size: opts.size };
    if (opts.checked !== undefined) def.checked = opts.checked;
    if (opts.defaultChecked !== undefined) def.defaultChecked = opts.defaultChecked;
    if (opts.onValue !== undefined) def.onValue = opts.onValue;
    if (opts.checkStyle !== undefined) def.checkStyle = opts.checkStyle;
    this.defs.push(def);
    this.names.add(name);
    return this as unknown as FormBuilder<
      S & Record<N, { type: "checkbox"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
    >;
  }

  // -------------------------------------------------------------------------
  // addRadioGroup
  // -------------------------------------------------------------------------

  addRadioGroup<N extends string, O extends string>(
    name: N,
    opts: Omit<RadioGroupOptions, "selected" | "defaultSelected"> & { options: readonly (RadioOption & { value: O })[]; selected?: NoInfer<O>; defaultSelected?: NoInfer<O> },
  ): FormBuilder<
    S & Record<N, { type: "radio"; readOnly: boolean; value: string | null; states: readonly O[]; options: readonly [] }>
  > {
    if (!name) throw new Error("Field name must be non-empty");
    if (this.names.has(name)) throw new Error(`Duplicate field name: "${name}"`);
    if (opts.options.length === 0) throw new RangeError(`${name}: radio group must have at least one option`);

    // Validate option values are unique
    const seen = new Set<string>();
    for (const opt of opts.options) {
      if (seen.has(opt.value)) throw new Error(`${name}: duplicate radio option value: "${opt.value}"`);
      seen.add(opt.value);
      assertFinite(opt.page, `${name} option(${opt.value}).page`);
      assertFinite(opt.x, `${name} option(${opt.value}).x`);
      assertFinite(opt.y, `${name} option(${opt.value}).y`);
      assertPositive(opt.size, `${name} option(${opt.value}).size`);
    }

    if (opts.selected !== undefined && !seen.has(opts.selected)) {
      throw new RangeError(`${name}: selected "${opts.selected}" not in options`);
    }
    if (opts.defaultSelected !== undefined && !seen.has(opts.defaultSelected)) {
      throw new RangeError(`${name}: defaultSelected "${opts.defaultSelected}" not in options`);
    }

    const def: WireRadioGroup = {
      type: "radioGroup",
      name,
      options: opts.options.map((o) => ({ value: o.value, page: o.page, x: o.x, y: o.y, size: o.size })),
    };
    if (opts.selected !== undefined) def.selected = opts.selected;
    if (opts.defaultSelected !== undefined) def.defaultSelected = opts.defaultSelected;
    if (opts.required !== undefined) def.required = opts.required;
    if (opts.readOnly !== undefined) def.readOnly = opts.readOnly;
    if (opts.tooltip !== undefined) def.tooltip = opts.tooltip;
    if (opts.checkStyle !== undefined) def.checkStyle = opts.checkStyle;
    this.defs.push(def);
    this.names.add(name);
    return this as unknown as FormBuilder<
      S & Record<N, { type: "radio"; readOnly: boolean; value: string | null; states: readonly O[]; options: readonly [] }>
    >;
  }

  // -------------------------------------------------------------------------
  // addDropdown
  // -------------------------------------------------------------------------

  addDropdown<N extends string, O extends string>(
    name: N,
    opts: ChoiceOptions<O>,
  ): FormBuilder<
    S & Record<N, { type: "dropdown"; readOnly: boolean; value: string | null; states: readonly []; options: readonly O[] }>
  > {
    return this._addChoice(name, opts, true, opts.editable ?? false) as unknown as FormBuilder<
      S & Record<N, { type: "dropdown"; readOnly: boolean; value: string | null; states: readonly []; options: readonly O[] }>
    >;
  }

  // -------------------------------------------------------------------------
  // addListBox
  // -------------------------------------------------------------------------

  addListBox<N extends string, O extends string>(
    name: N,
    opts: ChoiceOptions<O>,
  ): FormBuilder<
    S & Record<N, { type: "listbox"; readOnly: boolean; value: string | null; states: readonly []; options: readonly O[] }>
  > {
    return this._addChoice(name, opts, false, false) as unknown as FormBuilder<
      S & Record<N, { type: "listbox"; readOnly: boolean; value: string | null; states: readonly []; options: readonly O[] }>
    >;
  }

  private _addChoice<O extends string>(
    name: string,
    opts: ChoiceOptions<O>,
    combo: boolean,
    editable: boolean,
  ): this {
    const base = buildBase(name, opts, this.names);
    assertPositive(opts.width, `${name}.width`);
    assertPositive(opts.height, `${name}.height`);
    if (opts.options.length === 0) throw new RangeError(`${name}: choice field must have at least one option`);
    if (opts.multiSelect && combo) {
      throw new RangeError(`${name}: multiSelect is only valid on list boxes, not dropdowns`);
    }
    if (opts.selected !== undefined && !(opts.options as readonly string[]).includes(opts.selected)) {
      throw new RangeError(`${name}: selected "${opts.selected}" not in options`);
    }
    if (opts.defaultSelected !== undefined && !(opts.options as readonly string[]).includes(opts.defaultSelected)) {
      throw new RangeError(`${name}: defaultSelected "${opts.defaultSelected}" not in options`);
    }
    const def: WireChoice = {
      ...base,
      type: "choice",
      width: opts.width,
      height: opts.height,
      combo,
      options: [...opts.options],
    };
    if (editable && combo) def.editable = true;
    if (opts.multiSelect && !combo) def.multiselect = true;
    if (opts.selected !== undefined) def.selected = opts.selected;
    if (opts.defaultSelected !== undefined) def.defaultSelected = opts.defaultSelected;
    applyTextStyle(def, opts, name);
    this.defs.push(def);
    this.names.add(name);
    return this;
  }

  // -------------------------------------------------------------------------
  // addSignatureField
  // -------------------------------------------------------------------------

  addSignatureField<N extends string>(
    name: N,
    opts: SignatureFieldOptions,
  ): FormBuilder<
    S & Record<N, { type: "signature"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
  > {
    const base = buildBase(name, opts, this.names);
    assertPositive(opts.width, `${name}.width`);
    assertPositive(opts.height, `${name}.height`);
    const def: WireSignature = { ...base, type: "signature", width: opts.width, height: opts.height };
    this.defs.push(def);
    this.names.add(name);
    return this as unknown as FormBuilder<
      S & Record<N, { type: "signature"; readOnly: boolean; value: string | null; states: readonly []; options: readonly [] }>
    >;
  }

  // -------------------------------------------------------------------------
  // Accessors
  // -------------------------------------------------------------------------

  /** Return the names of all declared fields, typed to the schema. */
  getFieldNames(): FieldNameOf<S>[] {
    return [...this.names] as FieldNameOf<S>[];
  }
}
