import { describe, expect, test } from "bun:test";
import { PdfDocument, DuplicateAttachmentError, PdfError } from "../src/index.js";

const enc = new TextEncoder();

describe("attach() queueing", () => {
  test("duplicate queued name throws DuplicateAttachmentError at attach() time", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    doc.attach(enc.encode("a"), "same.txt");
    expect(() => doc.attach(enc.encode("b"), "same.txt")).toThrow(DuplicateAttachmentError);
  });

  test("attach is synchronous and does not mutate bytes before save", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    const base = await created.save();

    const doc = await PdfDocument.load(base);
    doc.attach(enc.encode("<x/>"), "data.xml");
    expect(await doc.getAttachments()).toEqual([]); // queued ≠ saved
  });

  test("getAttachments on an unsealed created doc returns []", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    expect(await doc.getAttachments()).toEqual([]);
  });
});

describe("round trip", () => {
  test("attach → save → load → getAttachments returns metadata and bytes", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    const base = await created.save();

    const doc = await PdfDocument.load(base);
    const payload = enc.encode("<invoice>42</invoice>");
    doc.attach(payload, "factur-x.xml", {
      mimeType: "text/xml",
      description: "Factur-X invoice data",
      creationDate: new Date(Date.UTC(2026, 0, 1, 12, 0, 0)),
      afRelationship: "Alternative",
    });
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts).toHaveLength(1);
    const a = atts[0]!;
    expect(a.name).toBe("factur-x.xml");
    expect(a.mimeType).toBe("text/xml");
    expect(a.description).toBe("Factur-X invoice data");
    expect(a.creationDate?.toISOString()).toBe("2026-01-01T12:00:00.000Z");
    expect(a.modificationDate).toBeUndefined();
    expect(a.afRelationship).toBe("Alternative");
    expect(a.size).toBe(payload.length);
    expect(Array.from(a.bytes)).toEqual(Array.from(payload));
  });

  test("attach on a created document is baked at save()", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    doc.attach(enc.encode("hello"), "note.txt");
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts.map((a) => a.name)).toEqual(["note.txt"]);
  });

  test("duplicate against the loaded document's tree throws at save", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("v1"), "same.txt");
    const withAtt = await created.save();

    const doc = await PdfDocument.load(withAtt);
    doc.attach(enc.encode("v2"), "same.txt");
    await expect(doc.save()).rejects.toThrow(DuplicateAttachmentError);
  });
});
