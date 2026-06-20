/** The 14 standard PDF fonts available without embedding. Text is limited to the WinAnsi charset. */
// Symbol and ZapfDingbats are intentionally not exposed: they use non-Latin
// encodings incompatible with the WinAnsi text model used by this library.
export enum StandardFonts {
  Helvetica = "Helvetica",
  HelveticaBold = "Helvetica-Bold",
  HelveticaOblique = "Helvetica-Oblique",
  HelveticaBoldOblique = "Helvetica-BoldOblique",
  Courier = "Courier",
  CourierBold = "Courier-Bold",
  CourierOblique = "Courier-Oblique",
  CourierBoldOblique = "Courier-BoldOblique",
  TimesRoman = "Times-Roman",
  TimesBold = "Times-Bold",
  TimesItalic = "Times-Italic",
  TimesBoldItalic = "Times-BoldItalic",
}
