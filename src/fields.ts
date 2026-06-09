import type { FieldInfo } from "./form.ts";

/** One queued mutation: set field `name` to `value`. */
export interface FillOp {
  name: string;
  value: string;
}

/** Shared, ordered list of pending mutations for a document. */
export class FillQueue {
  private readonly ops: FillOp[] = [];
  push(op: FillOp): void {
    this.ops.push(op);
  }
  toJSON(): string {
    return JSON.stringify(this.ops);
  }
  get length(): number {
    return this.ops.length;
  }
}

/** A text field. */
export class PdfTextField {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Set the field's text value. */
  setText(value: string): void {
    this.queue.push({ name: this.info.name, value });
  }
}

/** A checkbox. */
export class PdfCheckBox {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Check the box using its real on-state export value. */
  check(): void {
    const on = this.info.states[0];
    if (!on) throw new Error(`checkbox '${this.info.name}' has no on-state`);
    this.queue.push({ name: this.info.name, value: on });
  }
  /** Uncheck the box. */
  uncheck(): void {
    this.queue.push({ name: this.info.name, value: "Off" });
  }
}

/** A radio-button group. */
export class PdfRadioGroup {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid export values for this group. */
  get options(): string[] {
    return this.info.states;
  }
  /** Select an option by its real export value. */
  select(value: string): void {
    if (!this.info.states.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for radio '${this.info.name}' (valid: ${this.info.states.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}

/** A dropdown (choice) field. */
export class PdfDropdown {
  /** @internal */
  constructor(private readonly info: FieldInfo, private readonly queue: FillQueue) {}
  /** Valid option export values. */
  get options(): string[] {
    return this.info.options;
  }
  /** Select an option by its real export value. */
  select(value: string): void {
    if (this.info.options.length && !this.info.options.includes(value)) {
      throw new Error(
        `'${value}' is not a valid option for dropdown '${this.info.name}' (valid: ${this.info.options.join(", ")})`,
      );
    }
    this.queue.push({ name: this.info.name, value });
  }
}
