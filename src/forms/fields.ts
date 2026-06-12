import type { FieldInfo } from "./form.js";
import {
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
} from "../core/errors.js";

/** One queued mutation: set field `name` to a value or visual signature image. */
export type FillOp = {
  name: string;
  value: string;
} | {
  name: string;
  image: Uint8Array;
};

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
export class PdfTextField {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
export class PdfCheckBox {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
export class PdfRadioGroup<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
export class PdfDropdown<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
}

/**
 * A list-box choice field in a PDF form.
 *
 * `Opt` is the set of valid option values when the form is typed with generated
 * metadata. List boxes are single-select in this version.
 *
 * @example
 * ```ts
 * const language = form.getListBox("person.language");
 * language.select("TypeScript");
 * ```
 */
export class PdfListBox<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
export class PdfSignature {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
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
