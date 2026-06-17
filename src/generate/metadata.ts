/** User-facing document metadata shape. */
export interface DocumentMetadata {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string[];
  creator?: string;
  producer?: string;
  creationDate?: Date;
  modDate?: Date;
}

/**
 * Encode a `Date` as a PDF date string: `D:YYYYMMDDHHmmSSZ`.
 * Uses UTC getters so the result is timezone-independent.
 */
export function toPdfDate(d: Date): string {
  const pad2 = (n: number) => String(n).padStart(2, "0");
  const year = d.getUTCFullYear();
  const month = pad2(d.getUTCMonth() + 1);
  const day = pad2(d.getUTCDate());
  const hour = pad2(d.getUTCHours());
  const min = pad2(d.getUTCMinutes());
  const sec = pad2(d.getUTCSeconds());
  return `D:${year}${month}${day}${hour}${min}${sec}Z`;
}
