import type { FieldInfo } from "./form.js";
import {
  InvalidOptionError,
  MaxLengthExceededError,
  MissingOnStateError,
} from "./errors.js";

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

/** A text field. */
export class PdfTextField {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Set the field's text value. Throws if it exceeds the field's `/MaxLen`. */
  setText(value: string): void {
    const max = this.info.maxLength;
    if (max !== null && value.length > max) {
      throw new MaxLengthExceededError(this.info.name, max, value.length);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }
}

/** A checkbox. */
export class PdfCheckBox {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Check the box using its real on-state export value. */
  check(): void {
    const on = this.info.states[0];
    if (!on) throw new MissingOnStateError(this.info.name);
    this.queue.push({ name: this.info.name, value: on });
    this.info.value = on;
  }
  /** Uncheck the box. */
  uncheck(): void {
    this.queue.push({ name: this.info.name, value: "Off" });
    this.info.value = "Off";
  }
}

/** A radio-button group. `Opt` is its set of valid export values. */
export class PdfRadioGroup<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid export values for this group. */
  get options(): string[] {
    return this.info.states;
  }
  /** Select an option by its real export value. */
  select(value: Opt): void {
    if (!this.info.states.includes(value)) {
      throw new InvalidOptionError(this.info.name, "radio", value, this.info.states);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }
}

/** A dropdown (choice) field. `Opt` is its set of valid option values. */
export class PdfDropdown<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid option export values. */
  get options(): string[] {
    return this.info.options;
  }
  /** Select an option by its real export value. */
  select(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "dropdown", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }
}

/**
 * A list-box (choice) field. `Opt` is its set of valid option values. Like a
 * dropdown but rendered as a scrolling list; single-select only in this version.
 */
export class PdfListBox<Opt extends string = string> {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid option export values. */
  get options(): string[] {
    return this.info.options;
  }
  /** Select an option by its real export value. */
  select(value: Opt): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new InvalidOptionError(this.info.name, "listbox", value, this.info.options);
    }
    this.queue.push({ name: this.info.name, value });
    this.info.value = value;
  }
}

/** A visual signature field. This does not perform cryptographic signing. */
export class PdfSignature {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Set the signature's visual image. JPEG bytes are supported in this milestone. */
  setImage(bytes: Uint8Array): void {
    this.queue.push({ name: this.info.name, image: bytes.slice() });
  }
}
