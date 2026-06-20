import { expect, test } from "bun:test";
import { parseSvgPath, arcToCubics } from "../src/generate/svg-path.js";
import { PdfDocument, rgb } from "../src/index.js";

test("parses absolute M L Z", () => {
  expect(parseSvgPath("M10 20 L30 40 Z")).toEqual([
    {t:"m",x:10,y:20},{t:"l",x:30,y:40},{t:"z"},
  ]);
});
test("converts relative l to absolute", () => {
  expect(parseSvgPath("M10 10 l5 0")).toEqual([{t:"m",x:10,y:10},{t:"l",x:15,y:10}]);
});
test("converts H and V to line", () => {
  expect(parseSvgPath("M0 0 H10 V10")).toEqual([{t:"m",x:0,y:0},{t:"l",x:10,y:0},{t:"l",x:10,y:10}]);
});
test("converts quadratic Q to cubic c", () => {
  const segs = parseSvgPath("M0 0 Q5 10 10 0");
  expect(segs[0]).toEqual({t:"m",x:0,y:0});
  expect(segs[1]!.t).toBe("c"); // quadratic promoted to cubic
});
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
  const segs = parseSvgPath("M0 0 A5 5 0 1 1 10 0");
  const cubics = segs.filter((s) => s.t === "c");
  expect(cubics.length).toBeGreaterThan(1);
  // sweep=1 (CW in SVG y-down) from the leftmost point traces through y<0
  // (above the chord). largeArc is irrelevant here: a diameter-chord arc is
  // always 180°, so only sweep selects the side.
  const anyAbove = cubics.some((s) => s.t === "c" && (s.y1 < 0 || s.y2 < 0 || s.y < 0));
  expect(anyAbove).toBe(true);
});

test("largeArc is irrelevant for a diameter semicircle (both arcs are 180°)", () => {
  const c0 = parseSvgPath("M0 0 A5 5 0 0 1 10 0").filter((s) => s.t === "c");
  const c1 = parseSvgPath("M0 0 A5 5 0 1 1 10 0").filter((s) => s.t === "c");
  expect(c0.length).toBe(c1.length);
  for (let i = 0; i < c0.length; i++) {
    const a = c0[i]!, b = c1[i]!;
    if (a.t === "c" && b.t === "c") {
      expect(Math.abs(a.x - b.x)).toBeLessThan(1e-6);
      expect(Math.abs(a.y - b.y)).toBeLessThan(1e-6);
    }
  }
});

test("diameter semicircle with sweep=0 traces the opposite side", () => {
  const cubics = parseSvgPath("M0 0 A5 5 0 1 0 10 0").filter((s) => s.t === "c");
  expect(cubics.length).toBeGreaterThan(1);
  const anyBelow = cubics.some((s) => s.t === "c" && (s.y1 > 0 || s.y2 > 0 || s.y > 0));
  expect(anyBelow).toBe(true);
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
test("drawSvgPath round-trips into a valid PDF", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawSvgPath("M50 50 L150 50 L100 150 Z", { fill: rgb(1,0,0) });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
test("drawPolygon closed", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  page.drawPolygon([{x:10,y:10},{x:50,y:10},{x:30,y:40}], { stroke: rgb(0,0,0), closed: true });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
test("drawPolygon defaults to closed", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  // No `closed` option — should default to true (closed polygon)
  page.drawPolygon([{x:10,y:10},{x:50,y:10},{x:30,y:40}], { stroke: rgb(0,0,0) });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
test("drawSvgPath with an arc round-trips into a valid PDF", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage();
  // Half-circle arc plus a closing line — must not throw and must render.
  page.drawSvgPath("M50 100 A50 50 0 0 1 150 100 Z", { fill: rgb(0, 0, 1) });
  const out = await doc.save();
  expect((await PdfDocument.load(out)).getPageCount()).toBe(1);
});
