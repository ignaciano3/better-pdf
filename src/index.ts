import { roundTrip, readFields, fillFields, flattenFields } from "./wasm.js";
import { PdfForm } from "./form.js";

/**
 * A loaded PDF document. Holds the source bytes, exposes the AcroForm, and
 * persists queued field mutations on `save()` via an incremental update.
 */
export class PdfDocument {
  private form?: PdfForm;

  /** @internal */
  private constructor(private readonly bytes: Uint8Array) {}

  /**
   * Load a PDF from bytes. Async because later milestones (and the browser build)
   * initialize the WASM module asynchronously; callers should always `await`.
   */
  static async load(input: Uint8Array | ArrayBuffer): Promise<PdfDocument> {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    return new PdfDocument(bytes);
  }

  /**
   * Serialize back to PDF bytes, applying queued fills then flattens as
   * incremental updates. With nothing queued, returns a byte-exact round-trip.
   */
  async save(): Promise<Uint8Array> {
    const form = this.form;
    let bytes = this.bytes;
    if (form && form.queue.length > 0) {
      bytes = fillFields(bytes, form.queue.toJSON());
    }
    if (form && form.flattenQueue.length > 0) {
      bytes = flattenFields(bytes, JSON.stringify(form.flattenQueue));
    }
    if (bytes === this.bytes) {
      return roundTrip(this.bytes);
    }
    return bytes;
  }

  /**
   * The document's AcroForm. The same instance is returned each call, so queued
   * mutations accumulate until `save()`.
   */
  getForm(): PdfForm {
    if (!this.form) this.form = new PdfForm(this.bytes, readFields);
    return this.form;
  }
}

export { PdfForm } from "./form.js";
export type { FieldInfo, FieldType } from "./form.js";
export {
  PdfTextField,
  PdfCheckBox,
  PdfRadioGroup,
  PdfDropdown,
  PdfSignature,
} from "./fields.js";
export { generateFormTypes } from "./typegen.js";
export type { GenerateFormTypesOptions } from "./typegen.js";
