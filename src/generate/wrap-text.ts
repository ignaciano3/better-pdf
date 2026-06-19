/**
 * Word-wrap `text` so each line's measured width is `<= maxWidth`. Existing
 * `\n` characters are preserved as hard breaks; wrapping is applied within each
 * resulting paragraph. A single word wider than `maxWidth` is placed on its own
 * line (no mid-word breaking). Runs of spaces collapse to a single space.
 *
 * @param measure - returns the rendered width of a string at the caller's font/size.
 */
export function wrapText(
  text: string,
  maxWidth: number,
  measure: (s: string) => number,
): string {
  return text
    .split("\n")
    .map((para) => wrapParagraph(para, maxWidth, measure))
    .join("\n");
}

function wrapParagraph(
  para: string,
  maxWidth: number,
  measure: (s: string) => number,
): string {
  const words = para.split(/\s+/).filter((w) => w.length > 0);
  if (words.length === 0) return "";
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current === "" ? word : `${current} ${word}`;
    if (current === "" || measure(candidate) <= maxWidth) {
      current = candidate;
    } else {
      lines.push(current);
      current = word;
    }
  }
  if (current !== "") lines.push(current);
  return lines.join("\n");
}
