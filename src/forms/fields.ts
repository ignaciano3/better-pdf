import type { FieldInfo, FieldWidget } from "./form.js";
import {
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
  MultiSelectError,
} from "../core/errors.js";

/**
 * Flag changes for a loaded field. Each property toggles one PDF flag: `true`
 * sets it, `false` clears it, and omitting it leaves the flag unchanged.
 * `readOnly` / `required` / `noExport` are field `/Ff` flags; `hidden` /
 * `print` / `noView` are annotation `/F` flags applied to every widget.
 */
export interface FieldFlagChanges {
  readOnly?: boolean;
  required?: boolean;
  noExport?: boolean;
  hidden?: boolean;
  print?: boolean;
  noView?: boolean;
  /**
   * Appearance-affecting text-field `/Ff` flags. Toggling any of these on a
   * loaded field regenerates the field's appearance stream from its current
   * value. `combMaxLen` is the cell count written to `/MaxLen` when enabling
   * `comb`.
   */
  multiline?: boolean;
  password?: boolean;
  comb?: boolean;
  combMaxLen?: number;
}

/** One queued mutation: set field `name` to a value or visual signature image. */
export type FillOp =
  | { name: string; value: string }
  | { name: string; values: string[] }
  | { name: string; defaultValue: string }
  | { name: string; reset: true }
  | { name: string; image: Uint8Array }
  | { name: string; flags: FieldFlagChanges };

/** Shared, ordered list of pending mutations for a document. */
export class FillQueue {
  private readonly ops: FillOp[] = [];
  push(op: FillOp): void {
    this.ops.push(op);
  }
  /**
   * Serialize for the WASM boundary: image bytes are concatenated into one
   * binary blob; the JSON ops reference them by offset + length.
   */
  toPayload(): { opsJson: string; images: Uint8Array } {
    let total = 0;
    for (const op of this.ops) if ("image" in op) total += op.image.length;
    const images = new Uint8Array(total);
    let offset = 0;
    const wire = this.ops.map((op) => {
      if (!("image" in op)) return op;
      images.set(op.image, offset);
      const entry = { name: op.name, imageOffset: offset, imageLength: op.image.length };
      offset += op.image.length;
      return entry;
    });
    return { opsJson: JSON.stringify(wire), images };
  }
  get length(): number {
    return this.ops.length;
  }
}

/**
 * Common base for every form-field wrapper. Beyond holding the field's parsed
 * {@link FieldInfo} and the document's pending-mutation queue, it exposes the
 * setters that change a *loaded* field's flags (as opposed to its value): the
 * field `/Ff` flags ({@link setReadOnly}, {@link setRequired},
 * {@link setExported}) and the per-widget `/F` visibility flags ({@link hide},
 * {@link show}, {@link setPrintable}, {@link setNoView}).
 *
 * Every change is applied to the in-memory {@link FieldInfo} immediately and
 * written to the PDF bytes when `doc.save()` is called.
 */
export abstract class PdfField {
  /** @internal */
  constructor(protected readonly info: FieldInfo, protected readonly queue: FillQueue) {}

  /**
   * Set or clear this field's ReadOnly flag (`/Ff` bit 1). A read-only field is
   * displayed but cannot be edited or selected in a viewer.
   */
  setReadOnly(value: boolean): void {
    this.queue.push({ name: this.info.name, flags: { readOnly: value } });
    this.info.readOnly = value;
  }

  /**
   * Set or clear this field's Required flag (`/Ff` bit 2). Viewers may refuse to
   * submit the form while a required field is empty.
   */
  setRequired(value: boolean): void {
    this.queue.push({ name: this.info.name, flags: { required: value } });
    this.info.required = value;
  }

  /**
   * Set whether this field is exported when the form is submitted. `false` sets
   * the NoExport flag (`/Ff` bit 3); `true` clears it. `FieldInfo.exported`
   * mirrors this value (it is the inverse of the NoExport flag).
   */
  setExported(value: boolean): void {
    this.queue.push({ name: this.info.name, flags: { noExport: !value } });
    this.info.exported = value;
  }

  /**
   * Hide this field on screen and in print by setting the Hidden flag (`/F`
   * bit 2) on every one of its widgets.
   */
  hide(): void {
    this.setWidgetFlag({ hidden: true }, (w) => (w.hidden = true));
  }

  /** Show this field by clearing the Hidden flag (`/F` bit 2) on every widget. */
  show(): void {
    this.setWidgetFlag({ hidden: false }, (w) => (w.hidden = false));
  }

  /**
   * Set or clear the Print flag (`/F` bit 3) on every widget: whether the field
   * appears in printed output.
   */
  setPrintable(value: boolean): void {
    this.setWidgetFlag({ print: value }, (w) => (w.print = value));
  }

  /**
   * Set or clear the NoView flag (`/F` bit 6) on every widget: hidden on screen
   * but still rendered when printed (if also printable).
   */
  setNoView(value: boolean): void {
    this.setWidgetFlag({ noView: value }, (w) => (w.noView = value));
  }

  private setWidgetFlag(flags: FieldFlagChanges, mut: (w: FieldWidget) => void): void {
    this.queue.push({ name: this.info.name, flags });
    for (const w of this.info.widgets) mut(w);
  }
}

/**
 * A text field in a PDF form.
 *
 * Use `form.getTextField(name)` to get a `PdfTextField`, then call `setText()`
 * to queue a new value. The value is written to the PDF when `doc.save()` is
 * called.
 *
 * @example
 * ```ts
 * const name = form.getTextField("person.fullName");
 * name.setText("Ada Lovelace");
 *
 * const pdfBytes = await doc.save();
 * ```
 */
export class PdfTextField extends PdfField {
  /**
   * Set this field's text value.
   *
   * The field is updated in memory immediately, and the PDF bytes are updated
   * when you call `doc.save()`.
   *
   * @param value - The text to place in the field.
   * @throws `MaxLengthExceededError` when the value is longer than the field's
   * declared `/MaxLen`.
   *
   * @example
   * ```ts
   * form.getTextField("person.fullName").setText("Ada Lovelace");
   * ```
   */
  setText(value: string): void {
    const max = this.info.maxLength;
    if (max !== null && value.length > max) {
      throw new MaxLengthExceededError(this.info.name, max, value.length);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }

  /**
   * Set this field's default/reset value (`/DV`), independent of the current
   * value. A PDF viewer's "reset form" restores the field to this value.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   *
   * @param value - The default text.
   * @throws `MaxLengthExceededError` when the value is longer than the field's
   * declared `/MaxLen`.
   *
   * @example
   * ```ts
   * form.getTextField("invoice.currency").setDefaultText("USD");
   * ```
   */
  setDefaultText(value: string): void {
    const max = this.info.maxLength;
    if (max !== null && value.length > max) {
      throw new MaxLengthExceededError(this.info.name, max, value.length);
    }
    this.queue.push({ name: this.info.name, defaultValue: value });
    this.info.defaultValue = value;
  }

  /**
   * Set or clear this field's Multiline flag (`/Ff` bit 13). Unlike value
   * setters, this regenerates the field's appearance: a multiline field wraps
   * and top-aligns its current value; clearing it restores single-line layout.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   */
  setMultiline(value: boolean): void {
    this.queue.push({ name: this.info.name, flags: { multiline: value } });
    this.info.multiline = value;
  }

  /**
   * Set or clear this field's Comb flag (`/Ff` bit 25), which lays the value out
   * in `maxLen` fixed-pitch per-character cells. Enabling comb requires a cell
   * count, which is written to `/MaxLen`; clearing it leaves `/MaxLen` as-is.
   *
   * The field's appearance is regenerated, and the change is applied to the PDF
   * bytes when you call `doc.save()`.
   */
  setComb(value: true, maxLen: number): void;
  setComb(value: false): void;
  setComb(value: boolean, maxLen?: number): void {
    if (value) {
      this.queue.push({ name: this.info.name, flags: { comb: true, combMaxLen: maxLen } });
      this.info.comb = true;
      this.info.maxLength = maxLen ?? this.info.maxLength;
    } else {
      this.queue.push({ name: this.info.name, flags: { comb: false } });
      this.info.comb = false;
    }
  }

  /**
   * Set or clear this field's Password flag (`/Ff` bit 14). A password field's
   * value is never rendered into its appearance (the appearance is drawn empty),
   * though the `/V` value itself is preserved.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   */
  setPassword(value: boolean): void {
    this.queue.push({ name: this.info.name, flags: { password: value } });
    this.info.password = value;
  }
}

/**
 * A checkbox field in a PDF form.
 *
 * Use `check()` and `uncheck()` to queue the checkbox state. The state is
 * written to the PDF when `doc.save()` is called.
 *
 * @example
 * ```ts
 * const accepted = form.getCheckBox("person.acceptedTerms");
 * accepted.check();
 * ```
 */
export class PdfCheckBox extends PdfField {
  /**
   * Check this checkbox.
   *
   * The checkbox's real on-state export value is used automatically. You do not
   * need to know whether the PDF uses `"Yes"`, `"On"`, or another value.
   *
   * @throws `MissingOnStateError` when the checkbox has no declared on-state.
   *
   * @example
   * ```ts
   * form.getCheckBox("declaracion.acepta").check();
   * ```
   */
  check(): void {
    const on = this.info.states[0];
    if (!on) throw new MissingOnStateError(this.info.name);
    this.queue.push({ name: this.info.name, value: on });
    this.info.value = on;
  }
  /**
   * Uncheck this checkbox.
   *
   * @example
   * ```ts
   * form.getCheckBox("declaracion.acepta").uncheck();
   * ```
   */
  uncheck(): void {
    this.queue.push({ name: this.info.name, value: "Off" });
    this.info.value = "Off";
  }

  /**
   * Set this checkbox's default/reset state (`/DV`), independent of the current
   * state. A PDF viewer's "reset form" restores the checkbox to this state.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   *
   * @param checked - The default checked state.
   * @throws `MissingOnStateError` when setting the default to checked but the
   * checkbox has no declared on-state.
   *
   * @example
   * ```ts
   * form.getCheckBox("prefs.newsletter").setDefaultChecked(true);
   * ```
   */
  setDefaultChecked(checked: boolean): void {
    let value: string;
    if (checked) {
      const on = this.info.states[0];
      if (!on) throw new MissingOnStateError(this.info.name);
      value = on;
    } else {
      value = "Off";
    }
    this.queue.push({ name: this.info.name, defaultValue: value });
    this.info.defaultValue = value;
  }
}

/**
 * A radio-button group in a PDF form.
 *
 * `Opt` is the set of valid export values when the form is typed with generated
 * metadata. In untyped code, read `options` before selecting a value.
 *
 * @example
 * ```ts
 * const group = form.getRadioGroup("beneficiario.tipo_beneficiario");
 * console.log(group.options);
 * group.select("Titular");
 * ```
 */
export class PdfRadioGroup<Opt extends string = string> extends PdfField {
  /**
   * The valid export values for this radio group.
   *
   * Use one of these values with `select()`.
   *
   * @example
   * ```ts
   * const group = form.getRadioGroup("beneficiario.tipo_beneficiario");
   * for (const option of group.options) console.log(option);
   * ```
   */
  get options(): string[] {
    return this.info.states;
  }
  /**
   * Select one radio option by its real export value.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when `value` is not a valid option.
   *
   * @example
   * ```ts
   * const group = form.getRadioGroup("beneficiario.tipo_beneficiario");
   * group.select(group.options[0]);
   * ```
   */
  select(value: Opt): void {
    if (!this.info.states.includes(value)) {
      throw new InvalidOptionError(this.info.name, "radio", value, this.info.states);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }

  /**
   * Set this group's default/reset selection (`/DV`), independent of the current
   * selection. A PDF viewer's "reset form" restores the group to this option.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when `value` is not a valid option.
   *
   * @example
   * ```ts
   * const group = form.getRadioGroup("beneficiario.tipo_beneficiario");
   * group.setDefaultSelected("Titular");
   * ```
   */
  setDefaultSelected(value: Opt): void {
    if (!this.info.states.includes(value)) {
      throw new InvalidOptionError(this.info.name, "radio", value, this.info.states);
    }
    this.queue.push({ name: this.info.name, defaultValue: value });
    this.info.defaultValue = value;
  }
}

/**
 * A dropdown choice field in a PDF form.
 *
 * `Opt` is the set of valid option values when the form is typed with generated
 * metadata. In untyped code, read `options` before selecting a value.
 *
 * @example
 * ```ts
 * const status = form.getDropdown("beneficiario.estado_civil");
 * status.select("Casado");
 * ```
 */
export class PdfDropdown<Opt extends string = string> extends PdfField {
  /**
   * The valid option export values for this dropdown.
   *
   * Use one of these values with `select()`.
   *
   * @example
   * ```ts
   * const dropdown = form.getDropdown("beneficiario.estado_civil");
   * console.log(dropdown.options);
   * ```
   */
  get options(): string[] {
    return this.info.options;
  }
  /**
   * Select one dropdown option by its real export value.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when the dropdown declares options and `value`
   * is not one of them.
   *
   * @example
   * ```ts
   * const dropdown = form.getDropdown("beneficiario.estado_civil");
   * dropdown.select(dropdown.options[0]);
   * ```
   */
  select(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "dropdown", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }

  /**
   * Set this dropdown's default/reset value (`/DV`), independent of the current
   * value. A PDF viewer's "reset form" restores the dropdown to this option.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when the dropdown declares options and `value`
   * is not one of them.
   *
   * @example
   * ```ts
   * form.getDropdown("beneficiario.estado_civil").setDefaultSelected("Soltero");
   * ```
   */
  setDefaultSelected(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "dropdown", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, defaultValue: value });
    this.info.defaultValue = value;
  }
}

/**
 * A list-box choice field in a PDF form.
 *
 * `Opt` is the set of valid option values when the form is typed with generated
 * metadata. Use `select()` for single-select list boxes and `selectMultiple(values)`
 * for multi-select list boxes (those with the PDF Multiselect flag set,
 * i.e. `FieldInfo.multiSelect === true`).
 *
 * @example
 * ```ts
 * const language = form.getListBox("person.language");
 * language.select("TypeScript");
 * ```
 */
export class PdfListBox<Opt extends string = string> extends PdfField {
  /**
   * The valid option export values for this list box.
   *
   * Use one of these values with `select()`.
   *
   * @example
   * ```ts
   * const listBox = form.getListBox("person.language");
   * console.log(listBox.options);
   * ```
   */
  get options(): string[] {
    return this.info.options;
  }
  /**
   * Select one list-box option by its real export value.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when the list box declares options and `value`
   * is not one of them.
   *
   * @example
   * ```ts
   * const listBox = form.getListBox("person.language");
   * listBox.select(listBox.options[0]);
   * ```
   */
  select(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "listbox", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }

  /**
   * Set this list box's default/reset value (`/DV`), independent of the current
   * value. A PDF viewer's "reset form" restores the list box to this option.
   *
   * The change is applied to the PDF bytes when you call `doc.save()`.
   *
   * @param value - One of the values from `options`.
   * @throws `InvalidOptionError` when the list box declares options and `value`
   * is not one of them.
   *
   * @example
   * ```ts
   * form.getListBox("person.language").setDefaultSelected("TypeScript");
   * ```
   */
  setDefaultSelected(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "listbox", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, defaultValue: value });
    this.info.defaultValue = value;
  }

  /**
   * Select multiple list-box options by their real export values.
   *
   * Only valid for multi-select list boxes (the PDF Multiselect flag). The
   * queued values are written as the field's `/V` array and `/I` index array
   * when `doc.save()` is called.
   *
   * @param values - Export values, each one of `options`.
   * @throws `MultiSelectError` when this list box is single-select.
   * @throws `InvalidOptionError` when any value is not a valid option.
   *
   * @example
   * ```ts
   * form.getListBox("person.languages").selectMultiple(["ES", "EN"]);
   * ```
   */
  selectMultiple(values: Opt[]): void {
    if (!this.info.multiSelect) {
      throw new MultiSelectError(this.info.name);
    }
    const unique = [...new Set(values)] as Opt[];
    if (this.info.options.length) {
      for (const v of unique) {
        if (!this.info.options.includes(v)) {
          throw new InvalidOptionError(this.info.name, "listbox", v, this.info.options);
        }
      }
    }
    this.queue.push({ name: this.info.name, values: unique });
    this.info.value = unique.join(", ");
  }
}

/**
 * A visual signature field in a PDF form.
 *
 * This places an image in the signature field's appearance. It does not create
 * a cryptographic digital signature.
 *
 * @example
 * ```ts
 * const image = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
 * form.getSignature("firma.titular").setImage(image);
 * ```
 */
export class PdfSignature extends PdfField {
  /**
   * Set the signature field's visual image.
   *
   * JPEG bytes and supported PNG bytes are accepted. The image is copied when
   * queued, so the input array can be reused or modified afterwards.
   *
   * @param bytes - The image bytes to place in the signature field.
   * @throws `PdfCoreError` from `doc.save()` when the image format is not
   * supported by the PDF core.
   *
   * @example
   * ```ts
   * const image = new Uint8Array(await Bun.file("signature.png").arrayBuffer());
   * form.getSignature("firma.titular").setImage(image);
   * ```
   */
  setImage(bytes: Uint8Array): void {
    this.queue.push({ name: this.info.name, image: bytes.slice() });
  }
}
