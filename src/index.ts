import { roundTrip } from "./wasm.ts";
import { PdfForm } from "./form.ts";

/**
 * A loaded PDF document. In Milestone 1 it simply holds the original bytes and
 * round-trips them through the Rust/WASM core. Later milestones add a parsed
 * document model and form operations.
 */
export class PdfDocument {
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

  /** Serialize the document back to PDF bytes. */
  async save(): Promise<Uint8Array> {
    return roundTrip(this.bytes);
  }

  /** Read the document's AcroForm fields. */
  getForm(): PdfForm {
    return new PdfForm(this.bytes);
  }
}

export { PdfForm } from "./form.ts";
export type { FieldInfo, FieldType } from "./form.ts";
