// Runtime-neutral subpath entry: drawing types and helpers without
// PdfDocument or any WASM import. PdfDocument comes from the package root
// (or /browser) entry.
export { PdfPage } from "./page.js";
export type { DrawTextOptions } from "./page.js";
export { StandardFonts } from "./fonts.js";
export { rgb, grayscale } from "./color.js";
export type { Color } from "./color.js";
export { PageOutOfRangeError } from "../core/errors.js";
