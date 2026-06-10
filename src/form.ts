import {
  FillQueue,
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfListBox,
  PdfSignature,
} from "./fields.js";
import { UnknownFieldError, FieldTypeError } from "./errors.js";

export type FieldType =
  | "text" | "checkbox" | "radio" | "dropdown"
  | "listbox" | "signature" | "pushbutton" | "unknown";

export interface FieldWidget {
  /** 0-based page index the widget is on. */
  page: number;
  /** `/Rect` `[x0, y0, x1, y1]` in PDF points (origin bottom-left). */
  rect: [number, number, number, number];
}

export interface FieldInfo {
  /** Fully-qualified field name (ancestor /T joined by "."). */
  name: string;
  type: FieldType;
  /** Current value as a string, or null if unset. */
  value: string | null;
  /** On-state export values for checkbox/radio; empty otherwise. */
  states: string[];
  /** Option export values for dropdown/listbox; empty otherwise. */
  options: string[];
  readOnly: boolean;
  /** True if the field has the Required flag set. */
  required: boolean;
  /** One entry per widget annotation (page + position). Usually one; radio
   * groups and fields repeated across pages have several. */
  widgets: FieldWidget[];
}

export type ReadFields = (bytes: Uint8Array) => string;

/** A view over a PDF's AcroForm fields, with typed mutation accessors. */
export class PdfForm {
  private readonly fields: FieldInfo[];
  /** @internal — shared with PdfDocument so save() can flush pending ops. */
  readonly queue = new FillQueue();
  /** @internal — fully-qualified names queued for flattening on save. */
  readonly flattenQueue: string[] = [];

  /** @internal */
  constructor(bytes: Uint8Array, readFields: ReadFields) {
    this.fields = JSON.parse(readFields(bytes)) as FieldInfo[];
  }

  getFields(): FieldInfo[] {
    return this.fields;
  }

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

  getTextField(name: string): PdfTextField {
    return new PdfTextField(this.require(name, "text"), this.queue);
  }
  getCheckBox(name: string): PdfCheckBox {
    return new PdfCheckBox(this.require(name, "checkbox"), this.queue);
  }
  getRadioGroup(name: string): PdfRadioGroup {
    return new PdfRadioGroup(this.require(name, "radio"), this.queue);
  }
  getDropdown(name: string): PdfDropdown {
    return new PdfDropdown(this.require(name, "dropdown"), this.queue);
  }
  getListBox(name: string): PdfListBox {
    return new PdfListBox(this.require(name, "listbox"), this.queue);
  }
  getSignature(name: string): PdfSignature {
    return new PdfSignature(this.require(name, "signature"), this.queue);
  }

  /** Queue a single field to be flattened on save. */
  flattenField(name: string): void {
    if (!this.getField(name)) throw new UnknownFieldError(name);
    if (!this.flattenQueue.includes(name)) this.flattenQueue.push(name);
  }

  /** Queue all fields to be flattened on save. */
  flatten(): void {
    for (const f of this.fields) {
      if (!this.flattenQueue.includes(f.name)) this.flattenQueue.push(f.name);
    }
  }
}
