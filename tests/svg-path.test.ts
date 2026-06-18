import { expect, test } from "bun:test";
import { parseSvgPath } from "../src/generate/svg-path.js";
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
test("rejects arc commands", () => {
  expect(() => parseSvgPath("M0 0 A5 5 0 0 1 10 10")).toThrow();
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
