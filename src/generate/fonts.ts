/** The 14 standard PDF fonts available without embedding. Text is limited to the WinAnsi charset. */
// Symbol and ZapfDingbats are deliberately omitted: they have no WinAnsi text
// semantics. Revisit in M24 if requested.
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
