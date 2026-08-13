/** The 14 standard PDF fonts available without embedding. Text is limited to the WinAnsi charset. */
// Symbol and ZapfDingbats are intentionally not exposed: they use non-Latin
// encodings incompatible with the WinAnsi text model used by this library.
export enum StandardFonts {
  /** Helvetica regular. */
  Helvetica = "Helvetica",
  /** Helvetica bold. */
  HelveticaBold = "Helvetica-Bold",
  /** Helvetica oblique (italic). */
  HelveticaOblique = "Helvetica-Oblique",
  /** Helvetica bold-oblique. */
  HelveticaBoldOblique = "Helvetica-BoldOblique",
  /** Courier regular. */
  Courier = "Courier",
  /** Courier bold. */
  CourierBold = "Courier-Bold",
  /** Courier oblique (italic). */
  CourierOblique = "Courier-Oblique",
  /** Courier bold-oblique. */
  CourierBoldOblique = "Courier-BoldOblique",
  /** Times Roman regular. */
  TimesRoman = "Times-Roman",
  /** Times Roman bold. */
  TimesBold = "Times-Bold",
  /** Times Roman italic. */
  TimesItalic = "Times-Italic",
  /** Times Roman bold-italic. */
  TimesBoldItalic = "Times-BoldItalic",
}
