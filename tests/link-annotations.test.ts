import { expect, test } from "bun:test";
import { PdfDocument } from "../src/index.js";
import { join } from "path";

const FIXTURE = join(import.meta.dir, "fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");

test("URI link on created doc: round-trip valid", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage([612, 792]);
  page.drawLink({ x: 50, y: 50, width: 150, height: 30, url: "https://example.com" });
  const bytes = await doc.save();

  const reloaded = await PdfDocument.load(bytes);
  expect(reloaded.getPageCount()).toBe(1);
  await reloaded.save(); // second save to confirm still valid
});

test("URI link on loaded doc: round-trip valid", async () => {
  const fixture = await Bun.file(FIXTURE).arrayBuffer();
  const doc = await PdfDocument.load(new Uint8Array(fixture));
  const originalCount = doc.getPageCount();
  const page = doc.getPage(0);
  page.drawLink({ x: 50, y: 50, width: 200, height: 40, url: "https://example.com" });
  const bytes = await doc.save();

  const reloaded = await PdfDocument.load(bytes);
  expect(reloaded.getPageCount()).toBe(originalCount);
});

test("goToPage link: round-trip valid", async () => {
  const doc = await PdfDocument.create();
  const p0 = doc.addPage([612, 792]);
  doc.addPage([612, 792]);
  p0.drawLink({ x: 10, y: 10, width: 100, height: 20, goToPage: 1 });
  const bytes = await doc.save();

  const reloaded = await PdfDocument.load(bytes);
  expect(reloaded.getPageCount()).toBe(2);
});

test("drawLink throws when neither url nor goToPage provided", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage([612, 792]);
  expect(() => page.drawLink({ x: 10, y: 10, width: 100, height: 20 })).toThrow(
    "drawLink requires exactly one of `url` or `goToPage`",
  );
});

test("drawLink throws when both url and goToPage provided", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage([612, 792]);
  expect(() =>
    page.drawLink({ x: 10, y: 10, width: 100, height: 20, url: "https://example.com", goToPage: 0 }),
  ).toThrow("drawLink requires exactly one of `url` or `goToPage`");
});

test("drawLink throws on invalid width (zero)", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage([612, 792]);
  expect(() => page.drawLink({ x: 10, y: 10, width: 0, height: 20, url: "https://example.com" })).toThrow(
    RangeError,
  );
});

test("drawLink throws on non-finite coordinate", async () => {
  const doc = await PdfDocument.create();
  const page = doc.addPage([612, 792]);
  expect(() =>
    page.drawLink({ x: NaN, y: 10, width: 100, height: 20, url: "https://example.com" }),
  ).toThrow(RangeError);
});
