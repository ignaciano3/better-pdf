/** Common page sizes in PDF points (1 pt = 1/72 inch), as [width, height]. */
export const PageSizes = {
  /** ISO A3: 841.89 × 1190.55 pt. */
  A3: [841.89, 1190.55],
  /** ISO A4: 595.28 × 841.89 pt. */
  A4: [595.28, 841.89],
  /** ISO A5: 419.53 × 595.28 pt. */
  A5: [419.53, 595.28],
  /** US Letter: 612 × 792 pt. */
  Letter: [612, 792],
  /** US Legal: 612 × 1008 pt. */
  Legal: [612, 1008],
  /** US Tabloid: 792 × 1224 pt. */
  Tabloid: [792, 1224],
} as const satisfies Record<string, readonly [number, number]>;

/** A page size as a [width, height] tuple in PDF points. */
export type PageSize = readonly [number, number];
