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
 * Convert a single SVG elliptical arc (endpoint parametrization) to a sequence
 * of cubic-bézier {@link CurveSegment}s.
 *
 * Implements SVG 1.1 Appendix F.6.5 (endpoint -> center parametrization) and
 * F.6.6 (out-of-range radii correction). The sweep is split into segments of at
 * most 90 degrees, each approximated by one cubic bézier using the standard
 * `k = 4/3 * tan(theta / 4)` control-point distance.
 *
 * Degenerate cases per spec:
 *  - rx == 0 or ry == 0  -> a single straight line to (x, y).
 *  - start point == end point -> empty array (the arc is a no-op).
 *
 * @param x0 start x (absolute, current point)
 * @param y0 start y (absolute, current point)
 * @param rx ellipse x-radius
 * @param ry ellipse y-radius
 * @param phiDeg x-axis-rotation of the ellipse, in degrees
 * @param largeArc large-arc-flag
 * @param sweep sweep-flag
 * @param x end x (absolute)
 * @param y end y (absolute)
 */
export function arcToCubics(
  x0: number,
  y0: number,
  rx: number,
  ry: number,
  phiDeg: number,
  largeArc: boolean,
  sweep: boolean,
  x: number,
  y: number,
): Segment[] {
  // Degenerate: zero-length arc -> nothing to draw (spec F.6.2).
  if (x0 === x && y0 === y) {
    return [];
  }

  // Use absolute radii (spec F.6.6 step 1).
  rx = Math.abs(rx);
  ry = Math.abs(ry);

  // Degenerate: zero radius -> straight line (spec F.6.2).
  if (rx === 0 || ry === 0) {
    return [{ t: "l", x, y }];
  }

  const phi = (phiDeg * Math.PI) / 180;
  const cosPhi = Math.cos(phi);
  const sinPhi = Math.sin(phi);

  // Step 1 (F.6.5): compute (x1', y1') — the midpoint distance in the rotated
  // coordinate system.
  const dx = (x0 - x) / 2;
  const dy = (y0 - y) / 2;
  const x1p = cosPhi * dx + sinPhi * dy;
  const y1p = -sinPhi * dx + cosPhi * dy;

  // Step (F.6.6): correct out-of-range radii.
  let rxSq = rx * rx;
  let rySq = ry * ry;
  const x1pSq = x1p * x1p;
  const y1pSq = y1p * y1p;
  const lambda = x1pSq / rxSq + y1pSq / rySq;
  if (lambda > 1) {
    const s = Math.sqrt(lambda);
    rx *= s;
    ry *= s;
    rxSq = rx * rx;
    rySq = ry * ry;
  }

  // Step 2 (F.6.5): compute (cx', cy') — center in the rotated system.
  let radicand =
    (rxSq * rySq - rxSq * y1pSq - rySq * x1pSq) /
    (rxSq * y1pSq + rySq * x1pSq);
  if (radicand < 0) radicand = 0; // guard against tiny negatives from rounding
  let coef = Math.sqrt(radicand);
  if (largeArc === sweep) coef = -coef;
  const cxp = (coef * (rx * y1p)) / ry;
  const cyp = (coef * -(ry * x1p)) / rx;

  // Step 3 (F.6.5): compute the center (cx, cy) in the original system.
  const cx = cosPhi * cxp - sinPhi * cyp + (x0 + x) / 2;
  const cy = sinPhi * cxp + cosPhi * cyp + (y0 + y) / 2;

  // Step 4 (F.6.5): compute the start angle theta1 and the sweep angle
  // deltaTheta.
  const ux = (x1p - cxp) / rx;
  const uy = (y1p - cyp) / ry;
  const vx = (-x1p - cxp) / rx;
  const vy = (-y1p - cyp) / ry;

  const angle = (ux1: number, uy1: number, ux2: number, uy2: number): number => {
    const dot = ux1 * ux2 + uy1 * uy2;
    const len = Math.sqrt((ux1 * ux1 + uy1 * uy1) * (ux2 * ux2 + uy2 * uy2));
    let a = Math.acos(Math.min(1, Math.max(-1, dot / len)));
    if (ux1 * uy2 - uy1 * ux2 < 0) a = -a;
    return a;
  };

  const theta1 = angle(1, 0, ux, uy);
  let deltaTheta = angle(ux, uy, vx, vy);

  if (!sweep && deltaTheta > 0) deltaTheta -= 2 * Math.PI;
  if (sweep && deltaTheta < 0) deltaTheta += 2 * Math.PI;

  // Degenerate case: when radicand == 0 the two possible arcs are equal-length
  // semicircles (chord == diameter). The `angle` function cannot determine the
  // correct sign from the cross product (it is zero for antiparallel vectors),
  // so it defaults to +π. In this case the large-arc flag selects the
  // "other" semicircle, which requires flipping the sign of deltaTheta after
  // the sweep adjustment above.
  if (coef === 0 && largeArc) deltaTheta = -deltaTheta;

  // Split the sweep into segments of at most 90 degrees (PI/2).
  const segCount = Math.max(1, Math.ceil(Math.abs(deltaTheta) / (Math.PI / 2)));
  const delta = deltaTheta / segCount;

  // Control-point distance factor for one segment of angle `delta`.
  const k = (4 / 3) * Math.tan(delta / 4);

  const segments: Segment[] = [];

  // Point on the (rotated, translated) ellipse at parameter angle t.
  const pointAt = (t: number): { x: number; y: number } => {
    const cosT = Math.cos(t);
    const sinT = Math.sin(t);
    const ex = rx * cosT;
    const ey = ry * sinT;
    return {
      x: cosPhi * ex - sinPhi * ey + cx,
      y: sinPhi * ex + cosPhi * ey + cy,
    };
  };

  // Derivative (tangent) on the ellipse at parameter angle t (before scaling by k).
  const derivAt = (t: number): { x: number; y: number } => {
    const cosT = Math.cos(t);
    const sinT = Math.sin(t);
    const dex = -rx * sinT;
    const dey = ry * cosT;
    return {
      x: cosPhi * dex - sinPhi * dey,
      y: sinPhi * dex + cosPhi * dey,
    };
  };

  let t = theta1;
  for (let s = 0; s < segCount; s++) {
    const tNext = t + delta;
    const p1 = pointAt(t);
    const p2 = pointAt(tNext);
    const d1 = derivAt(t);
    const d2 = derivAt(tNext);

    const c1x = p1.x + k * d1.x;
    const c1y = p1.y + k * d1.y;
    const c2x = p2.x - k * d2.x;
    const c2y = p2.y - k * d2.y;

    segments.push({
      t: "c",
      x1: c1x,
      y1: c1y,
      x2: c2x,
      y2: c2y,
      x: p2.x,
      y: p2.y,
    });
    t = tNext;
  }

  // Force the final endpoint to be exactly the requested (x, y) so the path
  // closes without floating-point drift.
  const lastSeg = segments[segments.length - 1];
  if (lastSeg && lastSeg.t === "c") {
    lastSeg.x = x;
    lastSeg.y = y;
  }

  return segments;
}

/**
 * Parse an SVG path `d` string and return an array of primitive segments.
 *
 * Supported commands: M/m, L/l, H/h, V/v, C/c, S/s, Q/q, T/t, Z/z, A/a.
 * Arc commands A/a are converted to cubic béziers. Relative (lowercase)
 * commands are converted to absolute coordinates. Q/q quadratics are promoted
 * to C/c cubics. S/s smooth cubics and T/t smooth quadratics are resolved via
 * control-point reflection.
 *
 * @throws {Error} on malformed / empty input.
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

  // Consume a single arc flag (0 or 1). SVG allows flags to be packed with no
  // delimiter (e.g. "01" means flag 0 then flag 1), so we peel one digit off the
  // front of the current token and push the remainder back as a new token.
  function consumeFlag(): boolean {
    if (i >= tokens.length) throw new Error("SVG path: unexpected end of data");
    const t = tokens[i]!;
    const first = t[0];
    if (first !== "0" && first !== "1") {
      throw new Error(`SVG path: expected arc flag (0 or 1), got "${t}"`);
    }
    if (t.length === 1) {
      i++;
    } else {
      // Leave the remainder for the next read (handles packed flags and packed
      // flag-then-coordinate like "0-25" -> flag 0, then "-25").
      tokens[i] = t.slice(1);
    }
    return first === "1";
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
      do {
        const rx = consumeNumber();
        const ry = consumeNumber();
        const xAxisRotation = consumeNumber();
        const largeArc = consumeFlag();
        const sweep = consumeFlag();
        const ex = consumeNumber();
        const ey = consumeNumber();
        const ax = rel ? cx + ex : ex;
        const ay = rel ? cy + ey : ey;
        const arcSegs = arcToCubics(cx, cy, rx, ry, xAxisRotation, largeArc, sweep, ax, ay);
        for (const seg of arcSegs) segments.push(seg);
        cx = ax;
        cy = ay;
        prevCtrlX = cx;
        prevCtrlY = cy;
        prevCmd = "A";
      } while (hasMoreCoords());
      prevCmd = "A";
      continue;
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
