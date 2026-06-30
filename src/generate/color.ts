/** An RGB color with components in 0..1. Create with {@link rgb} or {@link grayscale}. */
export interface Color {
  readonly red: number;
  readonly green: number;
  readonly blue: number;
}

function clamp01(v: number, name: string): number {
  if (!Number.isFinite(v) || v < 0 || v > 1) {
    throw new RangeError(`${name} must be in 0..1, got ${v}`);
  }
  return v;
}

/** Create an RGB color. Components are in 0..1. */
export function rgb(red: number, green: number, blue: number): Color {
  return {
    red: clamp01(red, "red"),
    green: clamp01(green, "green"),
    blue: clamp01(blue, "blue"),
  };
}

/** Create a gray color; 0 is black, 1 is white. */
export function grayscale(level: number): Color {
  const v = clamp01(level, "level");
  return { red: v, green: v, blue: v };
}

/** Convert a {@link Color} to the `[r, g, b]` tuple used on the WASM wire. */
export function colorToTuple(c: Color): [number, number, number] {
  return [c.red, c.green, c.blue];
}
