# SVG Arc Commands (A/a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support SVG elliptical-arc path commands `A`/`a` in `page.drawSvgPath()` by converting each arc to cubic-bézier segments in pure TypeScript, so arcs render correctly without any Rust/wasm change.

**Architecture:** All work happens in `src/generate/svg-path.ts`. The arc branch in `parseSvgPath` currently throws at line 96-98; replace it with a handler that resolves the 7 arc parameters (absolute or relative endpoint), converts the SVG endpoint parametrization to center parametrization (SVG 1.1 spec Appendix F.6.5) with out-of-range-radii correction (F.6.6), splits the sweep into ≤90° segments, and approximates each segment with one cubic bézier using `k = 4/3 * tan(theta/4)`. Each resulting cubic is pushed as the existing `CurveSegment` (`{t:"c", x1,y1,x2,y2,x,y}`) — the exact variant the `C` handler already emits — so the Rust core needs no change. The tokenizer is extended to split packed flag digits (e.g. `0 1` written as `01`) because the SVG grammar allows arc flags to be single digits with no delimiter.

**Tech Stack:** TypeScript (no Rust change), bun test.

## Global Constraints
- Only `src/generate/svg-path.ts` + `tests/svg-path.test.ts` + docs (`README.md`, `docs/site/src/content/docs/reference/limitations.md`, `CHANGELOG.md`) change.
- No Rust/wasm change; do NOT run `bun run build` for the unit tests on `parseSvgPath`.
- Bump the minor version to 0.16.0 in `package.json` — but COORDINATE: if multiple features ship tonight they may share one minor bump, so phrase the bump as "bump minor if not already bumped this cycle" (if `package.json` is already at `0.16.0` from another feature merged this cycle, do NOT bump again — just add the CHANGELOG entry under the existing `0.16.0` heading).
- Update `README.md` and `docs/site/src/content/docs/reference/limitations.md` to remove the "arc not supported / throws" caveat and list `A`/`a` among supported commands.
- Preserve all existing behavior: every currently-passing test in `tests/svg-path.test.ts` must still pass, EXCEPT the `"rejects arc commands"` test, which must be rewritten (an arc no longer throws).

---

## Task 1: Tokenizer handles packed arc flags

**Files:** `src/generate/svg-path.ts`, `tests/svg-path.test.ts`

**Interfaces:** No new exported interface. Internal change to the existing `tokenize(d: string): string[]` function. The flag-splitting is applied only when consuming the two arc flags (see Task 2's `consumeFlag()` helper), so `tokenize` itself stays unchanged and Task 2 introduces a `consumeFlag()` reader that peels a single `0`/`1` digit off the front of a numeric token, pushing the remainder back. This keeps the general tokenizer (which must treat `01` in a coordinate context as the number 1, but in a flag context as two flags `0` then `1`) correct.

Rationale: a numeric token like `01` is ambiguous — as a coordinate it is the number `1`, but as the two arc flags it is `0` then `1`. The only safe place to disambiguate is at flag-consumption time, inside the arc handler, where we know a single-digit flag (`0` or `1`) is expected. Therefore Task 1 adds NO production code; it only adds a regression test asserting the existing tokenizer still produces the tokens the arc handler will need, and documents the approach. The real flag splitting lands in Task 2.

### Steps

- [ ] **1.1 Write a documentation-only test for the tokenizer baseline.** Add this test to the end of `tests/svg-path.test.ts` (it asserts current behavior so we lock the baseline before changing the arc handler). This test exercises a private function only indirectly, so instead assert the public contract: a packed-flag arc string must not throw once Task 2 lands. For now, write a test that is EXPECTED TO FAIL (arc still throws):

```ts
test("packed arc flags parse without throwing", () => {
  // Flags written with no separator: large-arc-flag=0, sweep-flag=1 packed as "01"
  // is impossible to express un-packed here; use the spec example with a leading
  // negative coordinate that forces flag packing in real SVGs.
  expect(() => parseSvgPath("M0 0 a25 25 -30 0 1 50 -25")).not.toThrow();
});
```

- [ ] **1.2 Run and expect FAIL.** `bun test tests/svg-path.test.ts` — the `"packed arc flags parse without throwing"` test fails because arcs still throw (`SVG arc commands (A/a) are not supported`). This is expected; the full arc handler lands in Task 2, which also makes this pass. Leave this test in place.

- [ ] **1.3 Commit the failing test.**

```bash
git add tests/svg-path.test.ts
git commit -m "test(svg-path): add failing packed-arc-flag parse test"
```

---

## Task 2: Arc-to-cubic conversion and arc command handler

**Files:** `src/generate/svg-path.ts`, `tests/svg-path.test.ts`

**Interfaces:** Introduce one exported helper (exported so it can be unit-tested directly and reused) plus one internal flag reader:

```ts
/**
 * Convert a single SVG elliptical arc (endpoint parametrization) to a sequence
 * of cubic-bézier `CurveSegment`s. Implements SVG 1.1 Appendix F.6.5 (endpoint
 * to center conversion) and F.6.6 (out-of-range radii correction).
 *
 * @param x0,y0   current point (absolute, where the arc starts)
 * @param rx,ry   ellipse radii (already absolute values; may be 0)
 * @param phiDeg  x-axis-rotation of the ellipse, in degrees
 * @param largeArc large-arc-flag
 * @param sweep    sweep-flag
 * @param x,y     arc endpoint (absolute)
 * @returns cubic segments; a single line segment if rx==0 or ry==0; [] if start==end.
 */
export function arcToCubics(
  x0: number, y0: number,
  rx: number, ry: number,
  phiDeg: number,
  largeArc: boolean, sweep: boolean,
  x: number, y: number,
): Segment[]
```

The arc branch inside `parseSvgPath` calls `arcToCubics` once per arc parameter set, appending the returned segments to `segments`, then sets `cx/cy` to the endpoint and `prevCtrlX/prevCtrlY` to the endpoint (arcs do not participate in S/T reflection, so the previous-control point is reset to the current point — matching how L/H/V reset it).

### Steps

- [ ] **2.1 Write failing unit tests for arc parsing.** Replace the existing `"rejects arc commands"` test (lines 21-23 of `tests/svg-path.test.ts`) and add the new arc tests. Use this exact block in place of the old `"rejects arc commands"` test:

```ts
// Helper: euclidean distance for endpoint tolerance assertions
function dist(ax: number, ay: number, bx: number, by: number): number {
  return Math.hypot(ax - bx, ay - by);
}

test("absolute A arc produces cubics ending at the endpoint", () => {
  const segs = parseSvgPath("M0 0 A5 5 0 0 1 10 0");
  expect(segs[0]).toEqual({ t: "m", x: 0, y: 0 });
  // Every arc-produced segment is a cubic
  for (let k = 1; k < segs.length; k++) expect(segs[k]!.t).toBe("c");
  const last = segs[segs.length - 1]!;
  expect(last.t).toBe("c");
  if (last.t === "c") {
    expect(dist(last.x, last.y, 10, 0)).toBeLessThan(1e-6);
  }
});

test("relative a arc resolves endpoint relative to current point", () => {
  const segs = parseSvgPath("M10 10 a5 5 0 0 1 10 0");
  const last = segs[segs.length - 1]!;
  expect(last.t).toBe("c");
  if (last.t === "c") {
    // endpoint = current(10,10) + (10,0) = (20,10)
    expect(dist(last.x, last.y, 20, 10)).toBeLessThan(1e-6);
  }
});

test("arc with rx=0 degenerates to a straight line", () => {
  const segs = parseSvgPath("M0 0 A0 5 0 0 1 10 0");
  expect(segs).toEqual([
    { t: "m", x: 0, y: 0 },
    { t: "l", x: 10, y: 0 },
  ]);
});

test("arc with coincident start and end emits no arc segments", () => {
  const segs = parseSvgPath("M0 0 A5 5 0 0 1 0 0");
  // Only the moveto; the arc start == end so it contributes nothing
  expect(segs).toEqual([{ t: "m", x: 0, y: 0 }]);
});

test("packed arc flags parse without throwing", () => {
  expect(() => parseSvgPath("M0 0 a25 25 -30 0 1 50 -25")).not.toThrow();
});

test("packed arc flags parse to the correct endpoint", () => {
  const segs = parseSvgPath("M0 0 a25 25 -30 0 1 50 -25");
  const last = segs[segs.length - 1]!;
  expect(last.t).toBe("c");
  if (last.t === "c") {
    expect(dist(last.x, last.y, 50, -25)).toBeLessThan(1e-6);
  }
});

test("large-arc sweep yields multiple cubics on the correct side", () => {
  // A near-full sweep (large-arc=1) of a circle r=5 from (0,0) to (10,0).
  // The sweep is > 90deg so it must be split into more than one cubic.
  const segs = parseSvgPath("M0 0 A5 5 0 1 1 10 0");
  const cubics = segs.filter((s) => s.t === "c");
  expect(cubics.length).toBeGreaterThan(1);
  // sweep-flag=1 (clockwise in SVG y-down terms); with large-arc the path bulges
  // through y > 0. Assert at least one control point has y > 0.
  const anyAbove = cubics.some((s) => s.t === "c" && (s.y1 > 0 || s.y2 > 0 || s.y > 0));
  expect(anyAbove).toBe(true);
});

test("arcToCubics directly: quarter circle endpoint", () => {
  const out = arcToCubics(0, 0, 5, 5, 0, false, true, 5, 5);
  expect(out.length).toBeGreaterThanOrEqual(1);
  const last = out[out.length - 1]!;
  expect(last.t).toBe("c");
  if (last.t === "c") {
    expect(dist(last.x, last.y, 5, 5)).toBeLessThan(1e-6);
  }
});
```

Also update the import on line 2 to bring in `arcToCubics`:

```ts
import { parseSvgPath, arcToCubics } from "../src/generate/svg-path.js";
```

- [ ] **2.2 Run and expect FAIL.** `bun test tests/svg-path.test.ts` — the new tests fail: `arcToCubics` is not exported (import error / undefined), and `parseSvgPath` still throws on `A`/`a`. Confirm the failure messages reference the missing export and the arc throw.

- [ ] **2.3 Implement `arcToCubics` and the arc handler in `src/generate/svg-path.ts`.**

First, add the `arcToCubics` function. Place it immediately after the `tokenize` function (after line 28, before `parseSvgPath`):

```ts
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
```

Next, add a `consumeFlag` reader inside `parseSvgPath`, alongside `consumeNumber` (after line 73). It peels a single `0`/`1` flag digit off the next token, handling packed flags (`01` -> `0`, leaving `1`):

```ts
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
```

Finally, replace the arc throw (lines 96-98):

```ts
    if (upper === "A") {
      throw new Error("SVG arc commands (A/a) are not supported");
    }
```

with the arc handler:

```ts
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
```

Note: `consumeFlag` may mutate `tokens[i]` to the remainder of a packed token; `hasMoreCoords()` already inspects `tokens[i]` and correctly treats the remainder (a number or another flag digit) as "more coords", so the implicit-repeat loop continues to work.

Also update the JSDoc on `parseSvgPath` (lines 30-40): change "Arc commands A/a throw an error." to "Arc commands A/a are converted to cubic béziers." and update the `@throws` line to drop "on arc commands".

- [ ] **2.4 Run and expect PASS.** `bun test tests/svg-path.test.ts` — all tests pass, including the rewritten arc tests and all pre-existing M/L/H/V/C/S/Q/T/Z tests. If any pre-existing test regressed, fix before proceeding (do not weaken the new tests).

- [ ] **2.5 Commit.**

```bash
git add src/generate/svg-path.ts tests/svg-path.test.ts
git commit -m "feat(svg-path): convert SVG arc commands (A/a) to cubic beziers"
```

---

## Task 3: End-to-end render test (drawSvgPath with an arc)

**Files:** `tests/svg-path.test.ts`

**Interfaces:** None new — exercises the public `page.drawSvgPath` path through to `doc.save()` / `PdfDocument.load`.

This task DOES require the wasm build because it round-trips a real PDF. Run `source ~/.cargo/env && bun run build` before running this test.

### Steps

- [ ] **3.1 Write the end-to-end test.** Append to `tests/svg-path.test.ts`:

```ts
test("drawSvgPath with an arc round-trips into a valid PDF", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  // Half-circle arc plus a closing line — must not throw and must render.
  page.drawSvgPath("M50 100 A50 50 0 0 1 150 100 Z", { fill: rgb(0, 0, 1) });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
```

- [ ] **3.2 Build, then run and expect PASS.**

```bash
source ~/.cargo/env && bun run build
bun test tests/svg-path.test.ts
```

All tests, including the new end-to-end arc render test, pass. (If `drawSvgPath` previously rejected arcs at a layer above `parseSvgPath`, this test surfaces it — but per the architecture, `drawSvgPath` delegates to `parseSvgPath`, so passing `parseSvgPath` should be sufficient. Investigate with superpowers:systematic-debugging if it fails.)

- [ ] **3.3 Commit.**

```bash
git add tests/svg-path.test.ts
git commit -m "test(svg-path): end-to-end drawSvgPath arc render round-trip"
```

---

## Task 4: Documentation, CHANGELOG, and version bump

**Files:** `README.md`, `docs/site/src/content/docs/reference/limitations.md`, `CHANGELOG.md`, `package.json`

**Interfaces:** None.

### Steps

- [ ] **4.1 Update `README.md`.** Make these exact edits:

  1. Line ~34 — change:
     > ...supports M/L/H/V/C/S/Q/T/Z) and `page.drawPolygon(points, ...)`... SVG arc commands (A/a) are not yet supported.

     to remove the trailing caveat and add arcs to the supported list:
     > ...supports M/L/H/V/C/S/Q/T/Z and arcs A/a) and `page.drawPolygon(points, ...)`...

     (Delete the sentence "SVG arc commands (A/a) are not yet supported.")

  2. Lines ~354-355 — change the blockquote:
     > Supported SVG commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
     > `Q`/`q`, `T`/`t`, `Z`/`z`. Arc commands (`A`/`a`) are **not yet supported** and throw.

     to:
     > Supported SVG commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
     > `Q`/`q`, `T`/`t`, `Z`/`z`, and `A`/`a` (elliptical arcs, converted to cubic béziers).

  3. Line ~529 — change:
     > `page.drawSvgPath(d: string, options): void` — ... supports M/L/H/V/C/S/Q/T/Z (arcs A/a throw)

     to:
     > `page.drawSvgPath(d: string, options): void` — ... supports M/L/H/V/C/S/Q/T/Z and A/a (arcs)

  4. Lines ~756-757 — remove the limitations bullet:
     > - SVG arc commands (`A`/`a`) are not yet supported by `drawSvgPath`/`drawPolygon`;
     >   path coordinates are PDF user space (y-up), so y-down artwork appears flipped.

     Replace it with a bullet that keeps the y-up note (which is still true) but drops the arc caveat:
     > - SVG path coordinates are PDF user space (y-up), so y-down artwork appears flipped.

- [ ] **4.2 Update `docs/site/src/content/docs/reference/limitations.md`.** At lines ~77-79, change:

     > - **SVG arc commands (`A`/`a`) are not yet supported** — they throw at call time.
     >     Supported commands: `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`, `S`/`s`,
     >     `Q`/`q`, `T`/`t`, `Z`/`z`.

     to:

     > - **Supported SVG commands:** `M`/`m`, `L`/`l`, `H`/`h`, `V`/`v`, `C`/`c`,
     >     `S`/`s`, `Q`/`q`, `T`/`t`, `Z`/`z`, and `A`/`a` (elliptical arcs are
     >     converted to cubic béziers).

- [ ] **4.3 Update `CHANGELOG.md`.** Add an entry under `## [Unreleased]` (or under the existing `## [0.16.0]` heading if this cycle already created one). Use:

```md
## [0.16.0] - 2026-06-20

### Added

- `page.drawSvgPath()` now supports SVG elliptical-arc commands `A`/`a`. Arcs are
  converted to cubic-bézier segments in TypeScript (SVG 1.1 Appendix F.6.5/F.6.6),
  including out-of-range-radii correction, ≤90° sweep splitting, packed-flag
  parsing, and the spec degenerate cases (zero radius → line, zero-length → no-op).
```

  Also update line ~58 of `CHANGELOG.md` (the historical feature-summary line that reads "SVG arcs (A/a) not yet supported.") — leave historical entries untouched if they describe a past release; only the new entry should reflect the new capability. (Do NOT rewrite history; the line at 58 is part of a prior release note. Add the new note rather than editing the old one.)

- [ ] **4.4 Bump version if not already bumped this cycle.** Check `package.json`:

```bash
grep '"version"' package.json
```

  If it reads `"0.15.0"`, bump to `0.16.0`:

```bash
npm version 0.16.0 --no-git-tag-version
```

  If it already reads `"0.16.0"` (another feature merged this cycle), do nothing — the CHANGELOG entry from 4.3 goes under the existing `0.16.0` heading.

- [ ] **4.5 Verify the full suite still passes.** (No rebuild needed unless 3.x changed since the last build.)

```bash
bun test
```

  Confirm zero failures with superpowers:verification-before-completion before committing.

- [ ] **4.6 Commit.**

```bash
git add README.md docs/site/src/content/docs/reference/limitations.md CHANGELOG.md package.json
git commit -m "docs: document SVG arc support; release 0.16.0"
```

---

## Completion

- [ ] All tasks committed.
- [ ] `bun test` is green.
- [ ] Per the "always merge to master" memory: merge this branch to master locally (skip the merge/PR options menu) once review passes. Do not push/tag unless the user asks.
