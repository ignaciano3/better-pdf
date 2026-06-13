/** Common page sizes in PDF points (1 pt = 1/72 inch), as [width, height]. */
export const PageSizes = {
  A3: [841.89, 1190.55],
  A4: [595.28, 841.89],
  A5: [419.53, 595.28],
  Letter: [612, 792],
  Legal: [612, 1008],
  Tabloid: [792, 1224],
} as const satisfies Record<string, readonly [number, number]>;

/** A page size as a [width, height] tuple in PDF points. */
export type PageSize = readonly [number, number];
