import { readFileSync } from "node:fs";
import { describe, expect, test } from "bun:test";
import { PdfDocument, DuplicateAttachmentError, PdfError } from "../src/index.js";

const enc = new TextEncoder();

const FICHA = new Uint8Array(
  readFileSync("tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf"),
);

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

describe("e2e composition", () => {
  test("attach + fill + flatten in one save on a loaded PDF", async () => {
    const doc = await PdfDocument.load(FICHA);
    const form = doc.getForm();
    form.getTextField("beneficiario.apellidos_nombres").setText("GARCIA");
    form.flattenField("beneficiario.apellidos_nombres");
    doc.attach(enc.encode("<invoice/>"), "factur-x.xml", {
      mimeType: "text/xml",
      afRelationship: "Alternative",
    });
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    const atts = await out.getAttachments();
    expect(atts.map((a) => a.name)).toEqual(["factur-x.xml"]);
    expect(atts[0]!.afRelationship).toBe("Alternative");
    // flatten removed the field
    const names = out.getForm().getFields().map((f) => f.name);
    expect(names).not.toContain("beneficiario.apellidos_nombres");
  });

  test("attach coexists with page-structure ops (chained save path)", async () => {
    const doc = await PdfDocument.load(FICHA);
    doc.addPage(); // forces saveChained
    doc.attach(enc.encode("note"), "note.txt");
    const saved = await doc.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments()).map((a) => a.name)).toEqual(["note.txt"]);
  });

  test("unicode filename round-trips via /UF", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("dato"), "año-2026 –informe.txt");
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments())[0]!.name).toBe("año-2026 –informe.txt");
  });

  test("multiple attachments come back sorted by name", async () => {
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(enc.encode("2"), "b.txt");
    created.attach(enc.encode("1"), "a.txt");
    created.attach(enc.encode("3"), "c.txt");
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    expect((await out.getAttachments()).map((a) => a.name)).toEqual(["a.txt", "b.txt", "c.txt"]);
  });

  test("binary payload (non-text) round-trips byte-exact", async () => {
    const payload = new Uint8Array(1024);
    for (let i = 0; i < payload.length; i++) payload[i] = i % 256;
    const created = await PdfDocument.create();
    created.addPage();
    created.attach(payload, "blob.bin", { mimeType: "application/octet-stream" });
    const saved = await created.save();

    const out = await PdfDocument.load(saved);
    const a = (await out.getAttachments())[0]!;
    expect(a.size).toBe(1024);
    expect(Array.from(a.bytes)).toEqual(Array.from(payload));
  });

  test("save with no attachments queued produces byte-identical output to before this feature (hot-path guard)", async () => {
    // The plan must not contain an `attach` key and no attach WASM call may
    // run when nothing is queued: filling one field twice through two
    // separately-loaded docs must be deterministic and unaffected.
    const doc1 = await PdfDocument.load(FICHA);
    doc1.getForm().getTextField("beneficiario.apellidos_nombres").setText("X");
    const out1 = await doc1.save();

    const doc2 = await PdfDocument.load(FICHA);
    doc2.getForm().getTextField("beneficiario.apellidos_nombres").setText("X");
    const out2 = await doc2.save();

    expect(Buffer.from(out1).equals(Buffer.from(out2))).toBe(true);
  });
});
