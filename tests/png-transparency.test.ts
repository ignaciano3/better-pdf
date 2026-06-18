import { describe, it, expect } from "bun:test";
import { PdfDocument } from "../src/index.js";

// Minimal 1×1 RGBA PNG (semi-transparent pixel — same fixture as Rust tiny_rgba_png).
// Color: R=255 G=0 B=0 A=127 (semi-transparent red).
const TINY_RGBA_PNG = new Uint8Array([
  0x89,0x50,0x4e,0x47,0x0d,0x0a,0x1a,0x0a, 0x00,0x00,0x00,0x0d,0x49,0x48,0x44,0x52,
  0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01,0x08,0x06,0x00,0x00,0x00,0x1f,0x15,0xc4,
  0x89,0x00,0x00,0x00,0x0d,0x49,0x44,0x41,0x54,0x78,0xda,0x63,0xf8,0xcf,0xc0,0xf0,
  0x1f,0x00,0x05,0x00,0x01,0xff,0x89,0x99,0x3d,0x1d,0x00,0x00,0x00,0x00,0x49,0x45,
  0x4e,0x44,0xae,0x42,0x60,0x82,
]);

describe("PNG transparency", () => {
  it("RGBA PNG embeds, draws, saves, and reloads without error", async () => {
    const doc = await PdfDocument.create();
    const img = await doc.embedPng(TINY_RGBA_PNG);
    expect(img.width).toBe(1);
    expect(img.height).toBe(1);

    const page = doc.addPage([595, 842]);
    page.drawImage(img, { x: 100, y: 100, width: 100, height: 100 });

    const out = await doc.save();
    expect(out.length).toBeGreaterThan(0);

    // Round-trip: reload and verify the document is valid with 1 page.
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(1);
  });

  it("RGBA PNG output is non-trivially sized (SMask data present)", async () => {
    const doc = await PdfDocument.create();
    const img = await doc.embedPng(TINY_RGBA_PNG);
    doc.addPage([595, 842]);
    doc.getPage(0).drawImage(img, { x: 0, y: 0, width: 50, height: 50 });
    const out = await doc.save();

    // A PDF with a soft-mask (SMask) image XObject will contain the /SMask key.
    // Scan the raw bytes for the string "/SMask".
    const text = Buffer.from(out).toString("latin1");
    expect(text).toContain("/SMask");
  });
});
