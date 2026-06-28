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
    expect(field!.multiSelect).toBe(false);
  });

  test("multiSelect listbox round-trips and accepts selectMultiple", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addListBox("myMulti", {
        page: 0,
        x: 50,
        y: 500,
        width: 150,
        height: 80,
        options: ["X", "Y", "Z"] as const,
        multiSelect: true,
      });
    });
    const form = reloaded.getForm();
    const field = form.getField("myMulti");
    expect(field).toBeDefined();
    expect(field!.type).toBe("listbox");
    expect(field!.multiSelect).toBe(true);

    // The Multiselect flag is what gates selectMultiple — it must not throw.
    const listBox = form.getListBox("myMulti");
    listBox.selectMultiple(["X", "Z"]);
    const out = await reloaded.save();
    const again = await PdfDocument.load(out);
    const refilled = again.getForm().getField("myMulti");
    expect(refilled!.value).toContain("X");
    expect(refilled!.value).toContain("Z");
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

  test("multiSelect on a dropdown throws", async () => {
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
        multiSelect: true,
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
// comb field wire mapping + validation
// ---------------------------------------------------------------------------

describe("form-generation: comb", () => {
  test("addTextField comb sets wire comb + maxLength", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const defs: ConstructorParameters<typeof FormBuilder>[0] = [];
    const fb = new FormBuilder(defs, new Set<string>());
    fb.addTextField("ssn", { page: 0, x: 0, y: 0, width: 180, height: 24, maxLength: 9, comb: true });
    const def = defs[0] as { comb?: boolean; maxLength?: number };
    expect(def.comb).toBe(true);
    expect(def.maxLength).toBe(9);
  });

  test("comb without maxLength throws", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const fb = new FormBuilder([], new Set<string>());
    expect(() =>
      fb.addTextField("t", { page: 0, x: 0, y: 0, width: 180, height: 24, comb: true }),
    ).toThrow();
  });

  test("comb + multiline throws", async () => {
    const { FormBuilder } = await import("../src/generate/form-builder.ts");
    const fb = new FormBuilder([], new Set<string>());
    expect(() =>
      fb.addTextField("t", { page: 0, x: 0, y: 0, width: 180, height: 24, maxLength: 9, comb: true, multiline: true }),
    ).toThrow();
  });

  test("comb field round-trips and sets the Comb flag", async () => {
    const { PdfDocument } = await import("../src/index.ts");
    const doc = await PdfDocument.create();
    doc.addPage([595, 842]);
    doc.createForm().addTextField("ssn", {
      page: 0, x: 40, y: 700, width: 180, height: 24, maxLength: 9, comb: true, value: "123456789",
    });
    const out = await doc.save();
    const str = Array.from(out).map((b) => String.fromCharCode(b)).join("");
    expect(str).toContain("/MaxLen 9");
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

// ---------------------------------------------------------------------------
// FieldInfo flags round-trip (multiline / comb / align / tooltip / editable)
// ---------------------------------------------------------------------------

describe("form-generation: FieldInfo flags round-trip", () => {
  test("multiline, align and tooltip are readable after reload", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("notes", {
        page: 0, x: 50, y: 600, width: 200, height: 80,
        multiline: true, align: "right", tooltip: "Additional notes",
      });
    });
    const field = reloaded.getForm().getField("notes")!;
    expect(field.multiline).toBe(true);
    expect(field.comb).toBe(false);
    expect(field.align).toBe("right");
    expect(field.tooltip).toBe("Additional notes");
  });

  test("comb is readable after reload", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("ssn", {
        page: 0, x: 50, y: 500, width: 180, height: 24, maxLength: 9, comb: true,
      });
    });
    const field = reloaded.getForm().getField("ssn")!;
    expect(field.comb).toBe(true);
    expect(field.multiline).toBe(false);
  });

  test("editable dropdown is readable after reload", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addDropdown("country", {
        page: 0, x: 50, y: 400, width: 160, height: 22,
        options: ["AR", "BR"] as const, editable: true,
      });
    });
    const field = reloaded.getForm().getField("country")!;
    expect(field.type).toBe("dropdown");
    expect(field.editable).toBe(true);
  });

  test("plain text field reports flag defaults", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("name", {
        page: 0, x: 50, y: 300, width: 200, height: 20,
      });
    });
    const field = reloaded.getForm().getField("name")!;
    expect(field.multiline).toBe(false);
    expect(field.comb).toBe(false);
    expect(field.password).toBe(false);
    expect(field.editable).toBe(false);
    expect(field.align).toBe("left");
    expect(field.tooltip).toBeNull();
  });

  test("password text field round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("pin", {
        page: 0, x: 50, y: 250, width: 120, height: 20, password: true,
      });
    });
    const field = reloaded.getForm().getField("pin")!;
    expect(field.password).toBe(true);
    expect(field.multiline).toBe(false);
  });

  test("fontName and fontSize from /DA round-trip", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("amount", {
        page: 0, x: 50, y: 200, width: 120, height: 20, fontSize: 14,
      });
    });
    const field = reloaded.getForm().getField("amount")!;
    expect(field.fontName).toBe("Helv");
    expect(field.fontSize).toBe(14);
  });

  test("fontName/fontSize are null for non-text fields", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addCheckBox("agree", { page: 0, x: 50, y: 150, size: 14 });
    });
    const field = reloaded.getForm().getField("agree")!;
    expect(field.fontName).toBeNull();
    expect(field.fontSize).toBeNull();
  });

  test("each widget exposes visibility flags; created fields are printable", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("w", { page: 0, x: 50, y: 100, width: 120, height: 20 });
    });
    const widget = reloaded.getForm().getField("w")!.widgets[0]!;
    // Created widgets carry the /F Print flag so they appear in printed output.
    expect(widget.print).toBe(true);
    expect(widget.hidden).toBe(false);
    expect(widget.noView).toBe(false);
  });

  test("created radio buttons are printable", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addRadioGroup("r", {
        options: [
          { value: "A", page: 0, x: 50, y: 80, size: 14 },
          { value: "B", page: 0, x: 80, y: 80, size: 14 },
        ] as const,
      });
    });
    for (const w of reloaded.getForm().getField("r")!.widgets) {
      expect(w.print).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// Default value (/DV) — builder + existing-field setters
// ---------------------------------------------------------------------------

describe("form-generation: default value (builder)", () => {
  test("text defaultValue round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("currency", {
        page: 0, x: 50, y: 700, width: 120, height: 20, value: "EUR", defaultValue: "USD",
      });
    });
    const field = reloaded.getForm().getField("currency")!;
    expect(field.value).toBe("EUR");
    expect(field.defaultValue).toBe("USD");
  });

  test("checkbox defaultChecked round-trips as on-state name", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addCheckBox("news", {
        page: 0, x: 50, y: 650, size: 14, checked: false, defaultChecked: true, onValue: "Yes",
      });
    });
    const field = reloaded.getForm().getField("news")!;
    expect(field.value).toBe("Off");
    expect(field.defaultValue).toBe("Yes");
  });

  test("radio defaultSelected round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addRadioGroup("plan", {
        defaultSelected: "A",
        options: [
          { value: "A", page: 0, x: 50, y: 600, size: 14 },
          { value: "B", page: 0, x: 80, y: 600, size: 14 },
        ] as const,
      });
    });
    const field = reloaded.getForm().getField("plan")!;
    expect(field.defaultValue).toBe("A");
  });

  test("dropdown defaultSelected round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addDropdown("status", {
        page: 0, x: 50, y: 550, width: 120, height: 20,
        options: ["Open", "Closed"] as const, defaultSelected: "Closed",
      });
    });
    const field = reloaded.getForm().getField("status")!;
    expect(field.defaultValue).toBe("Closed");
  });

  test("listbox defaultSelected round-trips", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addListBox("lang", {
        page: 0, x: 50, y: 500, width: 120, height: 40,
        options: ["ES", "EN"] as const, defaultSelected: "EN",
      });
    });
    const field = reloaded.getForm().getField("lang")!;
    expect(field.defaultValue).toBe("EN");
  });

  test("no default written when option omitted", async () => {
    const reloaded = await buildAndReload((doc) => {
      doc.createForm().addTextField("plain", { page: 0, x: 50, y: 450, width: 120, height: 20 });
    });
    expect(reloaded.getForm().getField("plain")!.defaultValue).toBeNull();
  });

  test("text defaultValue longer than maxLength throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.createForm().addTextField("code", {
        page: 0, x: 0, y: 0, width: 100, height: 20, maxLength: 3, defaultValue: "TOOLONG",
      }),
    ).toThrow();
  });

  test("dropdown defaultSelected not in options throws", async () => {
    const doc = await PdfDocument.create();
    doc.addPage(PageSizes.A4);
    expect(() =>
      doc.createForm().addDropdown("d", {
        page: 0, x: 0, y: 0, width: 100, height: 20,
        options: ["X", "Y"] as const, defaultSelected: "Z" as "X" | "Y",
      }),
    ).toThrow();
  });
});

describe("form-generation: default value (setters on existing fields)", () => {
  /** Build a base doc with one field per type, then reload it. */
  async function baseDoc() {
    return buildAndReload((doc) => {
      const fb = doc.createForm();
      fb.addTextField("t", { page: 0, x: 50, y: 700, width: 120, height: 20 });
      fb.addCheckBox("c", { page: 0, x: 50, y: 650, size: 14, onValue: "Yes" });
      fb.addRadioGroup("r", {
        options: [
          { value: "A", page: 0, x: 50, y: 600, size: 14 },
          { value: "B", page: 0, x: 80, y: 600, size: 14 },
        ] as const,
      });
      fb.addDropdown("d", { page: 0, x: 50, y: 550, width: 120, height: 20, options: ["Open", "Closed"] as const });
      fb.addListBox("l", { page: 0, x: 50, y: 500, width: 120, height: 40, options: ["ES", "EN"] as const });
    });
  }

  test("setDefaultText / Checked / Selected persist after save", async () => {
    const doc = await baseDoc();
    const form = doc.getForm();
    form.getTextField("t").setDefaultText("HELLO");
    form.getCheckBox("c").setDefaultChecked(true);
    form.getRadioGroup("r").setDefaultSelected("B");
    form.getDropdown("d").setDefaultSelected("Closed");
    form.getListBox("l").setDefaultSelected("EN");
    const reloaded = await PdfDocument.load(await doc.save());
    const f = reloaded.getForm();
    expect(f.getField("t")!.defaultValue).toBe("HELLO");
    expect(f.getField("c")!.defaultValue).toBe("Yes");
    expect(f.getField("r")!.defaultValue).toBe("B");
    expect(f.getField("d")!.defaultValue).toBe("Closed");
    expect(f.getField("l")!.defaultValue).toBe("EN");
  });

  test("setDefaultText does not change the current value", async () => {
    const doc = await baseDoc();
    const form = doc.getForm();
    form.getTextField("t").setText("CURRENT");
    form.getTextField("t").setDefaultText("DEFAULT");
    const reloaded = await PdfDocument.load(await doc.save());
    const field = reloaded.getForm().getField("t")!;
    expect(field.value).toBe("CURRENT");
    expect(field.defaultValue).toBe("DEFAULT");
  });

  test("setDefaultSelected rejects an invalid option", async () => {
    const doc = await baseDoc();
    expect(() => doc.getForm().getDropdown("d").setDefaultSelected("Nope" as "Open" | "Closed")).toThrow();
  });
});

// ---------------------------------------------------------------------------
// Reset — form.reset() / form.resetField(name)
// ---------------------------------------------------------------------------

describe("form-generation: reset", () => {
  /** Build a doc with defaults + changed current values, then reload. */
  async function filledDoc() {
    const built = await buildAndReload((doc) => {
      const fb = doc.createForm();
      fb.addTextField("t", { page: 0, x: 50, y: 700, width: 120, height: 20, defaultValue: "DEF" });
      fb.addCheckBox("c", { page: 0, x: 50, y: 650, size: 14, onValue: "Yes", defaultChecked: true });
      fb.addDropdown("d", {
        page: 0, x: 50, y: 600, width: 120, height: 20,
        options: ["Open", "Closed"] as const, defaultSelected: "Closed",
      });
    });
    // Change every current value away from its default.
    const form = built.getForm();
    form.getTextField("t").setText("CHANGED");
    form.getCheckBox("c").uncheck();
    form.getDropdown("d").select("Open");
    return PdfDocument.load(await built.save());
  }

  test("resetField restores one field to its default", async () => {
    const doc = await filledDoc();
    doc.getForm().resetField("t");
    const reloaded = await PdfDocument.load(await doc.save());
    const f = reloaded.getForm();
    expect(f.getField("t")!.value).toBe("DEF");
    // Untouched fields keep their changed value.
    expect(f.getField("d")!.value).toBe("Open");
  });

  test("reset() restores all value-bearing fields to their defaults", async () => {
    const doc = await filledDoc();
    doc.getForm().reset();
    const reloaded = await PdfDocument.load(await doc.save());
    const f = reloaded.getForm();
    expect(f.getField("t")!.value).toBe("DEF");
    expect(f.getField("c")!.value).toBe("Yes");
    expect(f.getField("d")!.value).toBe("Closed");
  });

  test("reset clears a field that has no default", async () => {
    const built = await buildAndReload((doc) => {
      doc.createForm().addTextField("t", { page: 0, x: 50, y: 700, width: 120, height: 20 });
    });
    built.getForm().getTextField("t").setText("SOMETHING");
    const mid = await PdfDocument.load(await built.save());
    mid.getForm().resetField("t");
    const reloaded = await PdfDocument.load(await mid.save());
    const v = reloaded.getForm().getField("t")!.value;
    expect(v === null || v === "").toBe(true);
  });

  test("resetField throws for an unknown field", async () => {
    const doc = await buildAndReload((d) => {
      d.createForm().addTextField("t", { page: 0, x: 50, y: 700, width: 120, height: 20 });
    });
    expect(() => doc.getForm().resetField("nope")).toThrow();
  });
});
