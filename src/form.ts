import { readFields } from "./wasm.ts";
import {
  FillQueue,
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
} from "./fields.ts";

export type FieldType =
  | "text" | "checkbox" | "radio" | "dropdown"
  | "listbox" | "signature" | "pushbutton" | "unknown";

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
}

/** A view over a PDF's AcroForm fields, with typed mutation accessors. */
export class PdfForm {
  private readonly fields: FieldInfo[];
  /** @internal — shared with PdfDocument so save() can flush pending ops. */
  readonly queue = new FillQueue();

  /** @internal */
  constructor(bytes: Uint8Array) {
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
    if (!f) throw new Error(`no such field: ${name}`);
    if (f.type !== type) {
      throw new Error(`field '${name}' is a ${f.type}, not a ${type}`);
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
}
