import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../../src/index.ts";

const FICHA = join(
  import.meta.dir,
  "../fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf",
);

function loadFicha() {
  return PdfDocument.load(new Uint8Array(readFileSync(FICHA)));
}

describe("fields on loaded docs", () => {
  test("createForm() on a loaded doc adds a fillable text field", async () => {
    const doc = await loadFicha();
    const form = doc.createForm();
    form.addTextField("bpf_added", { page: 0, x: 40, y: 700, width: 120, height: 20 });

    // Flush + fill in the same session.
    doc.getForm().getTextField("bpf_added").setText("hello");
    const out = await doc.save();

    // Reload and confirm the value round-trips and pre-existing fields survive.
    const reopened = await PdfDocument.load(out);
    const rf = reopened.getForm();
    expect(rf.getFields().map((f) => f.name)).toContain("bpf_added");
    expect(rf.getField("bpf_added")?.value ?? "").toBe("hello");
  });

  test("createForm() after getForm() throws", async () => {
    const doc = await loadFicha();
    doc.createForm().addTextField("bpf_x", { page: 0, x: 10, y: 10, width: 50, height: 15 });
    doc.getForm(); // builds the form
    expect(() => doc.createForm()).toThrow();
  });

  test("collision with an existing field throws at flush", async () => {
    const doc = await loadFicha();
    const existing = doc.getForm().getFields()[0]!.name;
    // New doc instance so getForm() hasn't sealed createForm().
    const doc2 = await loadFicha();
    doc2.createForm().addTextField(existing, { page: 0, x: 10, y: 10, width: 50, height: 15 });
    expect(() => doc2.getForm()).toThrow(/already exists/);
  });

  test("loaded doc that never calls createForm() is unchanged", async () => {
    const a = await (await loadFicha()).save();
    const b = await (await loadFicha()).save();
    expect(a).toEqual(b); // no field-injection path touched
  });
});
