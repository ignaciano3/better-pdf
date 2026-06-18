/**
 * SVG path parser: tokenizes an SVG `d` attribute string and converts all
 * commands to the primitive segment types understood by the Rust core.
 */

/** A move-to segment: start a new subpath at (x, y). */
export type MoveSegment = { t: "m"; x: number; y: number };
/** A line-to segment: draw a line from the current point to (x, y). */
export type LineSegment = { t: "l"; x: number; y: number };
/** A cubic Bézier segment: two control points + endpoint. */
export type CurveSegment = { t: "c"; x1: number; y1: number; x2: number; y2: number; x: number; y: number };
/** A close-path segment: close the current subpath. */
export type CloseSegment = { t: "z" };

/** A primitive path segment emitted by {@link parseSvgPath}. */
export type Segment = MoveSegment | LineSegment | CurveSegment | CloseSegment;

/** Tokenize an SVG path `d` string into command letters and numeric tokens. */
function tokenize(d: string): string[] {
  // Match command letters or numbers (including sign, decimal, exponent)
  const re = /([MmLlHhVvCcSsQqTtZzAa])|([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)/g;
  const tokens: string[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(d)) !== null) {
    tokens.push(m[0]!);
  }
  return tokens;
}

/**
 * Parse an SVG path `d` string and return an array of primitive segments.
 *
 * Supported commands: M/m, L/l, H/h, V/v, C/c, S/s, Q/q, T/t, Z/z.
 * Arc commands A/a throw an error. Relative (lowercase) commands are
 * converted to absolute coordinates. Q/q quadratics are promoted to C/c
 * cubics. S/s smooth cubics and T/t smooth quadratics are resolved via
 * control-point reflection.
 *
 * @throws {Error} on arc commands or malformed / empty input.
 */
export function parseSvgPath(d: string): Segment[] {
  if (!d || d.trim() === "") {
    throw new Error("SVG path d attribute must not be empty");
  }

  const tokens = tokenize(d);
  if (tokens.length === 0) {
    throw new Error("SVG path d attribute produced no tokens");
  }

  const segments: Segment[] = [];

  // Current point
  let cx = 0;
  let cy = 0;
  // Subpath start (for Z)
  let sx = 0;
  let sy = 0;
  // Previous control point for S/T reflection (absolute)
  let prevCtrlX = 0;
  let prevCtrlY = 0;
  // Previous command letter (uppercase)
  let prevCmd = "";

  let i = 0;

  function consumeNumber(): number {
    if (i >= tokens.length) throw new Error("SVG path: unexpected end of data");
    const t = tokens[i++]!;
    const v = Number(t);
    if (!Number.isFinite(v)) throw new Error(`SVG path: expected number, got "${t}"`);
    return v;
  }

  // Returns whether there are more numeric tokens to consume (next token is not a command letter)
  function hasMoreCoords(): boolean {
    if (i >= tokens.length) return false;
    const t = tokens[i]!;
    // A command letter is a single character from the SVG command set
    return !/^[MmLlHhVvCcSsQqTtZzAa]$/.test(t);
  }

  while (i < tokens.length) {
    const token = tokens[i]!;
    // Skip non-command tokens that appear without a preceding command
    if (!/^[MmLlHhVvCcSsQqTtZzAa]$/.test(token)) {
      // This should not happen for well-formed paths at the start of a command
      throw new Error(`SVG path: unexpected token "${token}" (expected command letter)`);
    }
    i++;

    const cmd = token;
    const rel = cmd === cmd.toLowerCase() && cmd !== "z" && cmd !== "Z";
    const upper = cmd.toUpperCase();

    if (upper === "A") {
      throw new Error("SVG arc commands (A/a) are not supported");
    }

    if (upper === "Z") {
      segments.push({ t: "z" });
      cx = sx;
      cy = sy;
      prevCmd = upper;
      prevCtrlX = cx;
      prevCtrlY = cy;
      continue;
    }

    if (upper === "M") {
      // First coord pair is a move; subsequent pairs are implicit L
      let first = true;
      do {
        const x = consumeNumber();
        const y = consumeNumber();
        const ax = rel && !first ? cx + x : (rel ? cx + x : x);
        const ay = rel && !first ? cy + y : (rel ? cy + y : y);
        if (first) {
          cx = ax;
          cy = ay;
          sx = cx;
          sy = cy;
          segments.push({ t: "m", x: cx, y: cy });
          first = false;
        } else {
          // Implicit lineto after the first moveto pair
          cx = ax;
          cy = ay;
          segments.push({ t: "l", x: cx, y: cy });
        }
        prevCtrlX = cx;
        prevCtrlY = cy;
      } while (hasMoreCoords());
      prevCmd = "M";
      continue;
    }

    if (upper === "L") {
      do {
        const x = consumeNumber();
        const y = consumeNumber();
        cx = rel ? cx + x : x;
        cy = rel ? cy + y : y;
        segments.push({ t: "l", x: cx, y: cy });
        prevCtrlX = cx;
        prevCtrlY = cy;
      } while (hasMoreCoords());
      prevCmd = "L";
      continue;
    }

    if (upper === "H") {
      do {
        const x = consumeNumber();
        cx = rel ? cx + x : x;
        segments.push({ t: "l", x: cx, y: cy });
        prevCtrlX = cx;
        prevCtrlY = cy;
      } while (hasMoreCoords());
      prevCmd = "H";
      continue;
    }

    if (upper === "V") {
      do {
        const y = consumeNumber();
        cy = rel ? cy + y : y;
        segments.push({ t: "l", x: cx, y: cy });
        prevCtrlX = cx;
        prevCtrlY = cy;
      } while (hasMoreCoords());
      prevCmd = "V";
      continue;
    }

    if (upper === "C") {
      do {
        const x1 = consumeNumber();
        const y1 = consumeNumber();
        const x2 = consumeNumber();
        const y2 = consumeNumber();
        const x = consumeNumber();
        const y = consumeNumber();
        const ax1 = rel ? cx + x1 : x1;
        const ay1 = rel ? cy + y1 : y1;
        const ax2 = rel ? cx + x2 : x2;
        const ay2 = rel ? cy + y2 : y2;
        const ax = rel ? cx + x : x;
        const ay = rel ? cy + y : y;
        prevCtrlX = ax2;
        prevCtrlY = ay2;
        cx = ax;
        cy = ay;
        segments.push({ t: "c", x1: ax1, y1: ay1, x2: ax2, y2: ay2, x: cx, y: cy });
        prevCmd = "C";
      } while (hasMoreCoords());
      prevCmd = "C";
      continue;
    }

    if (upper === "S") {
      // Smooth cubic: reflect previous cubic's second control point about current point
      do {
        const x2 = consumeNumber();
        const y2 = consumeNumber();
        const x = consumeNumber();
        const y = consumeNumber();

        // Reflection of previous control point (or current point if previous wasn't C/S)
        const ax1 = prevCmd === "C" || prevCmd === "S"
          ? 2 * cx - prevCtrlX
          : cx;
        const ay1 = prevCmd === "C" || prevCmd === "S"
          ? 2 * cy - prevCtrlY
          : cy;
        const ax2 = rel ? cx + x2 : x2;
        const ay2 = rel ? cy + y2 : y2;
        const ax = rel ? cx + x : x;
        const ay = rel ? cy + y : y;
        prevCtrlX = ax2;
        prevCtrlY = ay2;
        cx = ax;
        cy = ay;
        segments.push({ t: "c", x1: ax1, y1: ay1, x2: ax2, y2: ay2, x: cx, y: cy });
        prevCmd = "S";
      } while (hasMoreCoords());
      prevCmd = "S";
      continue;
    }

    if (upper === "Q") {
      // Quadratic Bézier: convert to cubic
      // Given p0 = current point, qc = quadratic control, p1 = endpoint:
      //   cubic c1 = p0 + 2/3 * (qc - p0)
      //   cubic c2 = p1 + 2/3 * (qc - p1)
      do {
        const qx = consumeNumber();
        const qy = consumeNumber();
        const x = consumeNumber();
        const y = consumeNumber();

        const aqx = rel ? cx + qx : qx;
        const aqy = rel ? cy + qy : qy;
        const ax = rel ? cx + x : x;
        const ay = rel ? cy + y : y;

        const p0x = cx;
        const p0y = cy;
        const ax1 = p0x + (2 / 3) * (aqx - p0x);
        const ay1 = p0y + (2 / 3) * (aqy - p0y);
        const ax2 = ax + (2 / 3) * (aqx - ax);
        const ay2 = ay + (2 / 3) * (aqy - ay);

        prevCtrlX = aqx;
        prevCtrlY = aqy;
        cx = ax;
        cy = ay;
        segments.push({ t: "c", x1: ax1, y1: ay1, x2: ax2, y2: ay2, x: cx, y: cy });
        prevCmd = "Q";
      } while (hasMoreCoords());
      prevCmd = "Q";
      continue;
    }

    if (upper === "T") {
      // Smooth quadratic: reflect previous quadratic control point about current point
      do {
        const x = consumeNumber();
        const y = consumeNumber();

        // Reflection of previous quadratic control (or current point if previous wasn't Q/T)
        const aqx = prevCmd === "Q" || prevCmd === "T"
          ? 2 * cx - prevCtrlX
          : cx;
        const aqy = prevCmd === "Q" || prevCmd === "T"
          ? 2 * cy - prevCtrlY
          : cy;
        const ax = rel ? cx + x : x;
        const ay = rel ? cy + y : y;

        const p0x = cx;
        const p0y = cy;
        const ax1 = p0x + (2 / 3) * (aqx - p0x);
        const ay1 = p0y + (2 / 3) * (aqy - p0y);
        const ax2 = ax + (2 / 3) * (aqx - ax);
        const ay2 = ay + (2 / 3) * (aqy - ay);

        prevCtrlX = aqx;
        prevCtrlY = aqy;
        cx = ax;
        cy = ay;
        segments.push({ t: "c", x1: ax1, y1: ay1, x2: ax2, y2: ay2, x: cx, y: cy });
        prevCmd = "T";
      } while (hasMoreCoords());
      prevCmd = "T";
      continue;
    }

    throw new Error(`SVG path: unsupported command "${cmd}"`);
  }

  if (segments.length === 0) {
    throw new Error("SVG path produced no segments");
  }

  return segments;
}
