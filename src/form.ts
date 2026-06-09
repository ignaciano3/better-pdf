import { readFields } from "./wasm.ts";

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

/** Read-only view over a PDF's AcroForm fields. */
export class PdfForm {
  private readonly fields: FieldInfo[];

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
}
