/** User-facing document metadata shape. */
export interface DocumentMetadata {
  /** Document title (`/Title`). */
  title?: string;
  /** Document author (`/Author`). */
  author?: string;
  /** Document subject (`/Subject`). */
  subject?: string;
  /** Document keywords (`/Keywords`). */
  keywords?: string[];
  /** Name of the software that created the document (`/Creator`). */
  creator?: string;
  /** Name of the software that produced the PDF (`/Producer`). */
  producer?: string;
  /** Creation date (`/CreationDate`). */
  creationDate?: Date;
  /** Last-modification date (`/ModDate`). */
  modificationDate?: Date;
}

/**
 * Encode a `Date` as a PDF date string: `D:YYYYMMDDHHmmSSZ`.
 * Uses UTC getters so the result is timezone-independent.
 */
export function toPdfDate(d: Date): string {
  const pad2 = (n: number) => String(n).padStart(2, "0");
  const year = String(d.getUTCFullYear()).padStart(4, "0");
  const month = pad2(d.getUTCMonth() + 1);
  const day = pad2(d.getUTCDate());
  const hour = pad2(d.getUTCHours());
  const min = pad2(d.getUTCMinutes());
  const sec = pad2(d.getUTCSeconds());
  return `D:${year}${month}${day}${hour}${min}${sec}Z`;
}

/**
 * Parse a PDF date string into a `Date`.
 *
 * Accepts the form `D:YYYYMMDDHHmmSS` optionally followed by `Z` or a UTC
 * offset like `+05'00'` or `-05'00'`. Year is the minimum required field;
 * missing month/day default to 1 and missing time parts default to 0.
 * Explicit offsets are applied to return a UTC instant. Returns `undefined`
 * for any unparseable input.
 */
export function fromPdfDate(s: string): Date | undefined {
  if (typeof s !== "string") return undefined;

  // Must start with "D:"
  if (!s.startsWith("D:")) return undefined;
  const rest = s.slice(2);

  // Extract numeric date/time parts: YYYYMMDDHHmmSS (year required, rest optional)
  const numMatch = rest.match(/^(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?(\d{2})?/);
  if (!numMatch || !numMatch[1]) return undefined;

  const year = parseInt(numMatch[1], 10);
  const month = numMatch[2] !== undefined ? parseInt(numMatch[2], 10) : 1;
  const day = numMatch[3] !== undefined ? parseInt(numMatch[3], 10) : 1;
  const hour = numMatch[4] !== undefined ? parseInt(numMatch[4], 10) : 0;
  const min = numMatch[5] !== undefined ? parseInt(numMatch[5], 10) : 0;
  const sec = numMatch[6] !== undefined ? parseInt(numMatch[6], 10) : 0;

  // Basic range validation
  if (month < 1 || month > 12) return undefined;
  if (day < 1 || day > 31) return undefined;
  if (hour > 23 || min > 59 || sec > 59) return undefined;

  // Parse optional timezone suffix that follows the numeric part
  const numLen = (numMatch[0] ?? "").length;
  const tzStr = rest.slice(numLen);

  let offsetMinutes = 0;
  if (tzStr === "" || tzStr === "Z") {
    offsetMinutes = 0;
  } else {
    // Expect +HH'mm' or -HH'mm'
    const tzMatch = tzStr.match(/^([+-])(\d{2})'(\d{2})'/);
    if (!tzMatch) return undefined;
    const sign = tzMatch[1] === "+" ? 1 : -1;
    const tzHour = parseInt(tzMatch[2]!, 10);
    const tzMin = parseInt(tzMatch[3]!, 10);
    offsetMinutes = sign * (tzHour * 60 + tzMin);
  }

  // Build UTC ms: treat the parsed fields as local-to-offset time, subtract offset
  const utcMs =
    Date.UTC(year, month - 1, day, hour, min, sec) - offsetMinutes * 60_000;

  const result = new Date(utcMs);
  if (isNaN(result.getTime())) return undefined;
  return result;
}
