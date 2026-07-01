import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes } from "../src/index.ts";

describe("getForm on created docs", () => {
  test("create -> getForm -> read fields (build-time values)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20, value: "Ada",
    });

    const form = doc.getForm();
    const field = form.getField("name");
    expect(field).toBeDefined();
    expect(field!.type).toBe("text");
    expect(field!.value).toBe("Ada");
  });

  test("create -> getForm -> setText -> save -> reload round-trips", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });

    doc.getForm().getTextField("name").setText("Grace Hopper");
    const out = await doc.save();

    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")!.value).toBe("Grace Hopper");
  });

  test("getForm returns the same instance and does not re-materialize", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20,
    });
    const a = doc.getForm();
    const b = doc.getForm();
    expect(a).toBe(b);
  });

  test("create -> save without getForm still works (opt-in materialization)", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    doc.createForm().addTextField("name", {
      page: 0, x: 50, y: 700, width: 200, height: 20, value: "baked",
    });
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getForm().getField("name")!.value).toBe("baked");
  });
});
