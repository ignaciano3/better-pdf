import { describe, expect, test } from "bun:test";
import { PdfDocument, PageSizes, PdfError } from "../src/index.ts";

const FICHA = "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf";

/** Build a single-page created doc, apply cb to its FormBuilder, save, and reload. */
async function buildAndReload(
  cb: (doc: ReturnType<typeof PdfDocument.create> extends Promise<infer T> ? T : never) => void,
) {
  const doc = await PdfDocument.create();
  doc.addPage(PageSizes.A4);
  cb(doc);
  const out = await doc.save();
  return PdfDocument.load(out);
}

// ---------------------------------------------------------------------------
// Field-type round-trips
// ---------------------------------------------------------------------------

describe("form-generation: text field", () => {
  test("text with value+maxLength round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("myText", {
        page: 0,
        x: 50,
        y: 700,
        width: 200,
        height: 20,
        value: "Hello",
        maxLength: 100,
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myText");
    expect(field).toBeDefined();
    expect(field!.type).toBe("text");
    expect(field!.value).toBe("Hello");
  });
});

describe("form-generation: checkbox", () => {
  test("checkbox checked with onValue round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addCheckBox("myCheck", {
        page: 0,
        x: 50,
        y: 650,
        size: 15,
        checked: true,
        onValue: "Yes",
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myCheck");
    expect(field).toBeDefined();
    expect(field!.type).toBe("checkbox");
    expect(field!.states).toContain("Yes");
    expect(field!.value).toBe("Yes");
  });
});

describe("form-generation: radio group", () => {
  test("radio 2 options, selected second, round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addRadioGroup("myRadio", {
        selected: "B",
        options: [
          { value: "A", page: 0, x: 50, y: 600, size: 12 },
          { value: "B", page: 0, x: 50, y: 580, size: 12 },
        ],
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myRadio");
    expect(field).toBeDefined();
    expect(field!.type).toBe("radio");
    expect(field!.states).toContain("A");
    expect(field!.states).toContain("B");
    expect(field!.value).toBe("B");
  });
});

describe("form-generation: dropdown", () => {
  test("dropdown options+selected round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addDropdown("myDrop", {
        page: 0,
        x: 50,
        y: 550,
        width: 150,
        height: 20,
        options: ["AR", "BR", "CL"] as const,
        selected: "BR",
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myDrop");
    expect(field).toBeDefined();
    expect(field!.type).toBe("dropdown");
    expect(field!.options).toContain("AR");
    expect(field!.options).toContain("BR");
    expect(field!.options).toContain("CL");
    expect(field!.value).toBe("BR");
  });
});

describe("form-generation: listbox", () => {
  test("listbox type round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addListBox("myList", {
        page: 0,
        x: 50,
        y: 500,
        width: 150,
        height: 60,
        options: ["X", "Y", "Z"] as const,
        selected: "Z",
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myList");
    expect(field).toBeDefined();
    expect(field!.type).toBe("listbox");
    expect(field!.options).toContain("Z");
    expect(field!.value).toBe("Z");
  });
});

describe("form-generation: signature", () => {
  test("signature field type round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addSignatureField("mySig", {
        page: 0,
        x: 50,
        y: 450,
        width: 200,
        height: 60,
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("mySig");
    expect(field).toBeDefined();
    expect(field!.type).toBe("signature");
  });
});

describe("form-generation: readOnly reflected", () => {
  test("readOnly flag is preserved on reload", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("roField", {
        page: 0,
        x: 50,
        y: 400,
        width: 150,
        height: 20,
        readOnly: true,
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("roField");
    expect(field).toBeDefined();
    expect(field!.readOnly).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// createForm on a loaded doc throws
// ---------------------------------------------------------------------------

describe("form-generation: guard on loaded doc", () => {
  test("createForm() on a loaded doc throws PdfError", async () => {
    const bytes = new Uint8Array(await Bun.file(FICHA).arrayBuffer());
    const doc = await PdfDocument.load(bytes);
    expect(() => doc.createForm()).toThrow(PdfError);
  });
});

// ---------------------------------------------------------------------------
// Validation errors
// ---------------------------------------------------------------------------

describe("form-generation: validation", () => {
  test("duplicate field name throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const fb = doc.createForm();
    fb.addTextField("dup", { page: 0, x: 10, y: 10, width: 100, height: 20 });
    expect(() =>
      fb.addTextField("dup", { page: 0, x: 10, y: 10, width: 100, height: 20 }),
    ).toThrow();
  });

  test("radio empty options throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const fb = doc.createForm();
    expect(() =>
      fb.addRadioGroup("r", { options: [] }),
    ).toThrow(RangeError);
  });

  test("choice selected not in options throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const fb = doc.createForm();
    expect(() =>
      fb.addDropdown("d", {
        page: 0,
        x: 10,
        y: 10,
        width: 100,
        height: 20,
        options: ["A", "B"] as const,
        selected: "Z" as "A",
      }),
    ).toThrow(RangeError);
  });

  test("width 0 throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    const fb = doc.createForm();
    expect(() =>
      fb.addTextField("t", { page: 0, x: 10, y: 10, width: 0, height: 20 }),
    ).toThrow(RangeError);
  });
});

// ---------------------------------------------------------------------------
// textColor wire mapping
// ---------------------------------------------------------------------------

describe("form-generation: textColor", () => {
  test("addTextField textColor maps to wire [r,g,b]", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const { rgb } = await import("../src/generate/color.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addTextField("t", { page: 0, x: 0, y: 0, width: 10, height: 10, textColor: rgb(1, 0, 0) });
    expect((defs[0] as { textColor?: number[] }).textColor).toEqual([1, 0, 0]);
  });

  test("addDropdown textColor maps to wire [r,g,b]", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const { rgb } = await import("../src/generate/color.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addDropdown("d", {
      page: 0, x: 0, y: 0, width: 10, height: 10,
      options: ["a"] as const, textColor: rgb(0, 0, 1),
    });
    expect((defs[0] as { textColor?: number[] }).textColor).toEqual([0, 0, 1]);
  });

  test("omitting textColor leaves wire def without it", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addTextField("t", { page: 0, x: 0, y: 0, width: 10, height: 10 });
    expect((defs[0] as { textColor?: number[] }).textColor).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// align + fontSize wire mapping
// ---------------------------------------------------------------------------

describe("form-generation: align + fontSize", () => {
  test("addTextField maps align + fontSize to wire", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addTextField("t", { page: 0, x: 0, y: 0, width: 10, height: 10, align: "center", fontSize: 18 });
    const def = defs[0] as { align?: string; fontSize?: number };
    expect(def.align).toBe("center");
    expect(def.fontSize).toBe(18);
  });

  test("addDropdown maps align + fontSize to wire", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addDropdown("d", {
      page: 0, x: 0, y: 0, width: 10, height: 10,
      options: ["a"] as const, align: "right", fontSize: 14,
    });
    const def = defs[0] as { align?: string; fontSize?: number };
    expect(def.align).toBe("right");
    expect(def.fontSize).toBe(14);
  });

  test("omitting align + fontSize leaves wire def without them", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addTextField("t", { page: 0, x: 0, y: 0, width: 10, height: 10 });
    const def = defs[0] as { align?: string; fontSize?: number };
    expect(def.align).toBeUndefined();
    expect(def.fontSize).toBeUndefined();
  });

  test("non-positive fontSize throws", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    expect(() =>
      fb.addTextField("t", { page: 0, x: 0, y: 0, width: 10, height: 10, fontSize: 0 }),
    ).toThrow();
  });
});

// ---------------------------------------------------------------------------
// checkStyle wire mapping
// ---------------------------------------------------------------------------

describe("form-generation: checkStyle", () => {
  test("addCheckBox maps checkStyle to wire", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addCheckBox("c", { page: 0, x: 0, y: 0, size: 12, checkStyle: "cross" });
    expect((defs[0] as { checkStyle?: string }).checkStyle).toBe("cross");
  });

  test("addRadioGroup maps checkStyle to wire", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addRadioGroup("r", {
      options: [{ value: "a", page: 0, x: 0, y: 0, size: 12 }] as const,
      checkStyle: "square",
    });
    expect((defs[0] as { checkStyle?: string }).checkStyle).toBe("square");
  });

  test("omitting checkStyle leaves wire def without it", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addCheckBox("c", { page: 0, x: 0, y: 0, size: 12 });
    expect((defs[0] as { checkStyle?: string }).checkStyle).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// editable combo box wire mapping
// ---------------------------------------------------------------------------

describe("form-generation: editable combo", () => {
  test("addDropdown editable:true sets wire editable + combo", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addDropdown("d", {
      page: 0, x: 0, y: 0, width: 10, height: 10,
      options: ["a"] as const, editable: true,
    });
    const def = defs[0] as { combo: boolean; editable?: boolean };
    expect(def.combo).toBe(true);
    expect(def.editable).toBe(true);
  });

  test("addDropdown without editable is not editable", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addDropdown("d", {
      page: 0, x: 0, y: 0, width: 10, height: 10, options: ["a"] as const,
    });
    expect((defs[0] as { editable?: boolean }).editable).toBeFalsy();
  });

  test("addListBox ignores editable (listbox cannot be combo)", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addListBox("l", {
      page: 0, x: 0, y: 0, width: 10, height: 10,
      options: ["a"] as const, editable: true,
    });
    const def = defs[0] as { combo: boolean; editable?: boolean };
    expect(def.combo).toBe(false);
    expect(def.editable).toBeFalsy();
  });
});

// ---------------------------------------------------------------------------
// getFieldNames returns declared names
// ---------------------------------------------------------------------------

describe("form-generation: getFieldNames", () => {
  test("getFieldNames returns all declared names", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const names = new Set<string>();
    const fb = new FormBuilder(defs, names);
    fb.addTextField("a", { page: 0, x: 0, y: 0, width: 10, height: 10 });
    fb.addCheckBox("b", { page: 0, x: 0, y: 0, size: 10 });
    // Cast to string[] — the static type is FieldNameOf<S> which TS narrows strictly.
    const result = fb.getFieldNames() as string[];
    expect(result).toContain("a");
    expect(result).toContain("b");
  });
});
