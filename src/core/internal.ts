/**
 * Package-internal Symbol keys for cross-class access to implementation details.
 *
 * Symbol-keyed properties do NOT appear in the public string-keyed .d.ts surface,
 * which lets sibling classes share state without polluting the public API.
 *
 * @internal
 */

/** Embedded-font id within the document's draw queue; undefined for standard-14. */
export const kFontId = Symbol("fontId");
/** Embedded-font bytes; undefined for standard-14. */
export const kFontBytes = Symbol("fontBytes");
/** Raw image bytes, embedded into the PDF at save time. */
export const kImageBytes = Symbol("imageBytes");
/** Raw source PDF bytes for an embedded page; embedded into the target PDF at save time. */
export const kEmbeddedBytes = Symbol("embeddedBytes");
/** PdfForm fill operations queue; accessed by PdfDocument at save time. */
export const kFormQueue = Symbol("formQueue");
/** PdfForm flatten queue; accessed by PdfDocument at save time. */
export const kFlattenQueue = Symbol("flattenQueue");
