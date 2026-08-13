import { fromPdfDate, toPdfDate } from "../generate/metadata.js";

/** /AFRelationship values (PDF 2.0 / PDF/A-3 associated files). */
export type AfRelationship =
  | "Source"
  | "Data"
  | "Alternative"
  | "Supplement"
  | "EncryptedPayload"
  | "FormData"
  | "Schema"
  | "Unspecified";

/** Options for {@link PdfDocumentBase.attach}. */
export interface AttachOptions {
  /** MIME type, written as the embedded stream's /Subtype (e.g. "text/xml"). */
  mimeType?: string;
  /** Human-readable description, written as the filespec /Desc. */
  description?: string;
  /** Written to /Params /CreationDate. Not defaulted (determinism). */
  creationDate?: Date;
  /** Written to /Params /ModDate. Not defaulted (determinism). */
  modificationDate?: Date;
  /**
   * Marks this file as an associated file: sets the filespec /AFRelationship
   * and appends it to the catalog /AF array (ZUGFeRD/Factur-X structure).
   */
  afRelationship?: AfRelationship;
}

/** One embedded file returned by {@link PdfDocumentBase.getAttachments}. */
export interface PdfAttachment {
  /** File name (as embedded). */
  name: string;
  /** Human-readable description, written as the filespec `/Desc`. */
  description?: string;
  /** MIME type, written as the embedded stream's `/Subtype`. */
  mimeType?: string;
  /** Creation date (`/Params /CreationDate`), if set. */
  creationDate?: Date;
  /** Modification date (`/Params /ModDate`), if set. */
  modificationDate?: Date;
  /** Uncompressed size in bytes (equals bytes.length). */
  size: number;
  /** Associated-file relationship (`/AFRelationship`), if any. */
  afRelationship?: AfRelationship;
  /** The embedded file's raw bytes. */
  bytes: Uint8Array;
}

/** @internal One queued attach() call. */
export interface QueuedAttachment {
  bytes: Uint8Array;
  name: string;
  options: AttachOptions;
}

/** @internal Wire entry read back from read_attachments. */
interface ReadEntry {
  name: string;
  description?: string;
  mimeType?: string;
  creationDate?: string;
  modificationDate?: string;
  afRelationship?: string;
  size: number;
  offset: number;
  length: number;
}

/** @internal Build the attach ops JSON + concatenated blob for the queue. */
export function toAttachPayload(queue: QueuedAttachment[]): {
  opsJson: string;
  blob: Uint8Array;
} {
  let total = 0;
  for (const q of queue) total += q.bytes.length;
  const blob = new Uint8Array(total);
  let offset = 0;
  const ops = queue.map((q) => {
    blob.set(q.bytes, offset);
    const op = {
      name: q.name,
      description: q.options.description,
      mimeType: q.options.mimeType,
      creationDate: q.options.creationDate && toPdfDate(q.options.creationDate),
      modificationDate: q.options.modificationDate && toPdfDate(q.options.modificationDate),
      afRelationship: q.options.afRelationship,
      offset,
      length: q.bytes.length,
    };
    offset += q.bytes.length;
    return op;
  });
  return { opsJson: JSON.stringify(ops), blob };
}

/** @internal Decode the packed `[u32 LE json_len][json][bytes]` buffer. */
export function decodeAttachments(packed: Uint8Array): PdfAttachment[] {
  const view = new DataView(packed.buffer, packed.byteOffset, packed.byteLength);
  const jsonLen = view.getUint32(0, true);
  const entries = JSON.parse(
    new TextDecoder().decode(packed.subarray(4, 4 + jsonLen)),
  ) as ReadEntry[];
  const blobStart = 4 + jsonLen;
  return entries.map((e) => ({
    name: e.name,
    description: e.description,
    mimeType: e.mimeType,
    creationDate: e.creationDate ? fromPdfDate(e.creationDate) : undefined,
    modificationDate: e.modificationDate ? fromPdfDate(e.modificationDate) : undefined,
    size: e.size,
    afRelationship: e.afRelationship as PdfAttachment["afRelationship"],
    bytes: packed.slice(blobStart + e.offset, blobStart + e.offset + e.length),
  }));
}
