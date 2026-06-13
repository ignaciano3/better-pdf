import { describe, expect, test } from "bun:test";
import { PdfDocument } from "../src/index.ts";
import { PageOutOfRangeError } from "../src/core/errors.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

async function load() {
  const bytes = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
  return PdfDocument.load(bytes);
}

describe("pages", () => {
  test("getPageCount and getPages", async () => {
    const doc = await load();
    const count = doc.getPageCount();
    expect(count).toBeGreaterThan(0);
    expect(doc.getPages()).toHaveLength(count);
  });

  test("page size and rotation", async () => {
    const doc = await load();
    const page = doc.getPage(0);
    expect(page.width).toBeGreaterThan(100);
    expect(page.height).toBeGreaterThan(100);
    expect(page.rotation % 90).toBe(0);
  });

  test("getPage out of range throws", async () => {
    const doc = await load();
    expect(() => doc.getPage(999)).toThrow(PageOutOfRangeError);
    expect(() => doc.getPage(-1)).toThrow(PageOutOfRangeError);
  });

  test("getPage returns same instance", async () => {
    const doc = await load();
    expect(doc.getPage(0)).toBe(doc.getPage(0));
  });
});
