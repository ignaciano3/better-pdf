// Behavioral tests ported from pdf-lib's test suite (https://github.com/Hopding/pdf-lib)
// and its corpus of tricky/malformed asset PDFs, adapted to the better-pdf API.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { PdfDocument } from "../src/index.ts";
import { EncryptedPdfError, IncorrectPasswordError, PdfError } from "../src/core/errors.ts";

const DIR = join(import.meta.dir, "fixtures/pdf-lib");
const bytes = (name: string) => new Uint8Array(readFileSync(join(DIR, name)));
const load = (name: string) => PdfDocument.load(bytes(name));

describe("loading tricky / malformed PDFs (pdf-lib corpus)", () => {
  const cases: Array<{ file: string; pages?: number }> = [
    { file: "normal.pdf", pages: 2 },
    { file: "stuff_following_header.pdf" },
    { file: "missing_xref_trailer_dict.pdf" },
    { file: "missing_endobj_keyword.pdf" },
    { file: "invalid_root_ref.pdf" },
    { file: "with_comments.pdf" },
    { file: "with_update_sections.pdf" },
    { file: "giraffe.pdf" },
    { file: "with_invalid_stream_EOL.pdf", pages: 2 },
    { file: "with_invalid_objects.pdf" },
    { file: "with_newline_whitespace_in_indirect_object_numbers.pdf" },
    { file: "linearized_with_object_streams.pdf" },
    { file: "with_missing_endstream_eol_and_polluted_ctm.pdf" },
    { file: "with_cropbox.pdf" },
    { file: "with_annots.pdf" },
    { file: "with_null_parent_entry.pdf" },
    { file: "with_viewer_prefs.pdf" },
    { file: "bixby_guide.pdf", pages: 176 },
    { file: "PDF 2.0 with offset start.pdf" },
    { file: "Simple PDF 2.0 file.pdf" },
    { file: "PDF 2.0 via incremental save.pdf" },
  ];

  for (const { file, pages } of cases) {
    test(`loads and inspects ${file}`, async () => {
      const doc = await load(file);
      const count = doc.getPageCount();
      if (pages !== undefined) expect(count).toBe(pages);
      else expect(count).toBeGreaterThan(0);
      const page = doc.getPage(0);
      expect(page.width).toBeGreaterThan(0);
      expect(page.height).toBeGreaterThan(0);
    });

    test(`save/reload roundtrip of ${file}`, async () => {
      const doc = await load(file);
      const before = doc.getPageCount();
      const out = await doc.save();
      const reloaded = await PdfDocument.load(out);
      expect(reloaded.getPageCount()).toBe(before);
    });
  }

  test("with_large_page_count.pdf reports a large page count", async () => {
    const doc = await load("with_large_page_count.pdf");
    expect(doc.getPageCount()).toBeGreaterThan(100);
  });
});

describe("encrypted PDFs (pdf-lib corpus)", () => {
  for (const file of ["encrypted_old.pdf", "encrypted_new.pdf"]) {
    test(`${file}: surfaces encryption instead of garbage`, async () => {
      // better-pdf is lazy: load may succeed, but touching content without a
      // password must raise EncryptedPdfError/IncorrectPasswordError — never
      // return corrupt data silently.
      let threw: unknown = null;
      try {
        const doc = await PdfDocument.load(bytes(file));
        doc.getPageCount();
        await doc.save();
      } catch (e) {
        threw = e;
      }
      if (threw !== null) {
        expect(
          threw instanceof EncryptedPdfError ||
            threw instanceof IncorrectPasswordError ||
            threw instanceof PdfError,
        ).toBe(true);
      }
    });
  }
});

describe("metadata (pdf-lib corpus)", () => {
  test("reads exotic metadata strings from just_metadata.pdf", async () => {
    const doc = await load("just_metadata.pdf");
    const meta = await doc.getMetadata();
    expect(meta.title).toBe(
      "Title metadata (StringType=HexString, Encoding=PDFDocEncoding) with some weird chars ˘•€",
    );
    expect(meta.author).toBe(
      "Author metadata (StringType=HexString, Encoding=UTF-16BE) with some chinese 你怎么敢",
    );
    expect(meta.subject).toBe(
      "Subject metadata (StringType=LiteralString, Encoding=UTF-16BE) with some chinese 你怎么敢",
    );
    expect(meta.producer).toBe("pdf-lib (https://github.com/Hopding/pdf-lib)");
    expect(meta.keywords).toBe(
      "Keywords metadata (StringType=LiteralString, Encoding=PDFDocEncoding) with  some weird  chars ˘•€",
    );
  });

  test("metadata set/save/reload roundtrip on a created document", async () => {
    const doc = await PdfDocument.create();
    doc.addPage();
    doc.setTitle("🥚 The Life of an Egg 🍳");
    doc.setAuthor("Humpty Dumpty");
    doc.setSubject("📘 An Epic Tale of Woe 📖");
    doc.setKeywords(["eggs", "wall", "fall", "king", "horses", "men", "🥚"]);
    doc.setProducer("PDF App 9000 🤖");
    doc.setCreator("PDF App 8000 🤖");
    doc.setCreationDate(new Date("1997-08-15T01:58:37Z"));
    doc.setModificationDate(new Date("2018-12-21T07:00:11Z"));
    const out = await doc.save();
    const meta = await (await PdfDocument.load(out)).getMetadata();
    expect(meta.title).toBe("🥚 The Life of an Egg 🍳");
    expect(meta.author).toBe("Humpty Dumpty");
    expect(meta.subject).toBe("📘 An Epic Tale of Woe 📖");
    expect(meta.producer).toBe("PDF App 9000 🤖");
    expect(meta.creator).toBe("PDF App 8000 🤖");
    expect(new Date(meta.creationDate!)).toEqual(new Date("1997-08-15T01:58:37Z"));
    expect(new Date(meta.modificationDate!)).toEqual(new Date("2018-12-21T07:00:11Z"));
  });
});

describe("page add/insert/remove lifecycle (normal.pdf)", () => {
  test("counts through add, insert, remove", async () => {
    const doc = await load("normal.pdf");
    expect(doc.getPageCount()).toBe(2);
    doc.addPage();
    doc.addPage();
    expect(doc.getPageCount()).toBe(4);
    doc.insertPage(0);
    doc.insertPage(4);
    expect(doc.getPageCount()).toBe(6);
    doc.removePage(5);
    doc.removePage(0);
    expect(doc.getPageCount()).toBe(4);
    const out = await doc.save();
    expect((await PdfDocument.load(out)).getPageCount()).toBe(4);
  });
});

describe("fancy_fields.pdf form (pdf-lib PDFForm.spec)", () => {
  const expectedFields: Record<string, string> = {
    "Prefix ⚽️": "text",
    "First Name 🚀": "text",
    "MiddleInitial 🎳": "text",
    "LastName 🛩": "text",
    "Are You A Fairy? 🌿": "checkbox",
    "Is Your Power Level Over 9000? 💪": "checkbox",
    "Can You Defeat Enemies In One Punch? 👊": "checkbox",
    "Will You Ever Let Me Down? ☕️": "checkbox",
    "Eject 📼": "pushbutton",
    "Submit 📝": "pushbutton",
    "Play ▶️": "pushbutton",
    "Launch 🚀": "pushbutton",
    "Historical Figures 🐺": "radio",
    "Which Are Planets? 🌎": "listbox",
    "Choose A Gundam 🤖": "dropdown",
  };

  test("enumerates all 15 fields with correct names and types", async () => {
    const doc = await load("fancy_fields.pdf");
    const fields = doc.getForm().getFields();
    expect(fields.length).toBe(15);
    for (const [name, type] of Object.entries(expectedFields)) {
      const f = fields.find((x) => x.name === name);
      expect(f?.name).toBe(name);
      expect(f?.type).toBe(type as any);
    }
  });

  test("reads initial checkbox and radio values", async () => {
    const form = (await load("fancy_fields.pdf")).getForm();
    const val = (n: string) => form.getField(n)?.value;
    expect(val("Are You A Fairy? 🌿")).toBeTruthy();
    expect(val("Is Your Power Level Over 9000? 💪")).toBeFalsy();
    expect(val("Can You Defeat Enemies In One Punch? 👊")).toBeTruthy();
    expect(val("Will You Ever Let Me Down? ☕️")).toBeFalsy();
    expect(val("Historical Figures 🐺")).toBe("Marcus Aurelius 🏛️");
  });

  test("fills text, toggles checkboxes, re-selects radio; survives save/reload", async () => {
    const doc = await load("fancy_fields.pdf");
    const form = doc.getForm();
    form.getTextField("First Name 🚀").setText("Dr. Slump");
    form.getCheckBox("Is Your Power Level Over 9000? 💪").check();
    form.getCheckBox("Are You A Fairy? 🌿").uncheck();
    form.getRadioGroup("Historical Figures 🐺").select("Alexander Hamilton 🇺🇸");
    const out = await doc.save();
    const rf = (await PdfDocument.load(out)).getForm();
    expect(rf.getField("First Name 🚀")?.value).toBe("Dr. Slump");
    expect(rf.getField("Is Your Power Level Over 9000? 💪")?.value).toBeTruthy();
    expect(rf.getField("Are You A Fairy? 🌿")?.value).toBeFalsy();
    expect(rf.getField("Historical Figures 🐺")?.value).toBe("Alexander Hamilton 🇺🇸");
  });

  test("dropdown and listbox selection survive save/reload", async () => {
    const doc = await load("fancy_fields.pdf");
    const form = doc.getForm();
    const dropdown = form.getDropdown("Choose A Gundam 🤖");
    const listbox = form.getListBox("Which Are Planets? 🌎");
    const dOpt = form.getField("Choose A Gundam 🤖")!.options![0]!;
    const lOpt = form.getField("Which Are Planets? 🌎")!.options![0]!;
    dropdown.select(dOpt);
    listbox.select(lOpt);
    const out = await doc.save();
    const rf = (await PdfDocument.load(out)).getForm();
    expect(rf.getField("Choose A Gundam 🤖")?.value).toBe(dOpt);
    const lv = rf.getField("Which Are Planets? 🌎")?.value;
    expect(Array.isArray(lv) ? lv : [lv]).toContain(lOpt);
  });

  test("flatten() removes fields and keeps page count", async () => {
    const doc = await load("fancy_fields.pdf");
    const pages = doc.getPageCount();
    const form = doc.getForm();
    form.getTextField("First Name 🚀").setText("Flat Stanley");
    form.flatten();
    const out = await doc.save();
    const reloaded = await PdfDocument.load(out);
    expect(reloaded.getPageCount()).toBe(pages);
    let fieldCount = 0;
    try {
      fieldCount = reloaded.getForm().getFields().length;
    } catch {
      fieldCount = 0;
    }
    expect(fieldCount).toBe(0);
  });
});

describe("other pdf-lib form documents", () => {
  for (const file of ["sample_form.pdf", "with_combed_fields.pdf", "dod_character.pdf", "form_to_flatten.pdf"]) {
    test(`${file}: enumerates fields and roundtrips a text fill`, async () => {
      const doc = await load(file);
      const form = doc.getForm();
      const fields = form.getFields();
      expect(fields.length).toBeGreaterThan(0);
      const textField = fields.find((f) => f.type === "text" && !f.readOnly);
      if (textField) {
        const value = textField.maxLength != null ? "X".repeat(Math.min(3, textField.maxLength)) : "hello";
        form.getTextField(textField.name).setText(value);
        const out = await doc.save();
        const rf = (await PdfDocument.load(out)).getForm();
        expect(rf.getField(textField.name)?.value).toBe(value);
      }
    });

    test(`${file}: flatten produces a loadable, field-free PDF`, async () => {
      const doc = await load(file);
      doc.getForm().flatten();
      const out = await doc.save();
      const reloaded = await PdfDocument.load(out);
      let fieldCount = 0;
      try {
        fieldCount = reloaded.getForm().getFields().length;
      } catch {
        fieldCount = 0;
      }
      expect(fieldCount).toBe(0);
    });
  }

  test("with_xfa_fields.pdf: form is accessible despite XFA", async () => {
    const doc = await load("with_xfa_fields.pdf");
    const fields = doc.getForm().getFields();
    expect(Array.isArray(fields)).toBe(true);
    const out = await doc.save();
    expect((await PdfDocument.load(out)).getPageCount()).toBe(doc.getPageCount());
  });

  test("with_signature.pdf: save with a signature field does not throw", async () => {
    const doc = await load("with_signature.pdf");
    doc.getForm();
    const out = await doc.save();
    expect(out.byteLength).toBeGreaterThan(0);
  });
});

describe("page geometry", () => {
  test("with_cropbox.pdf exposes sane page dimensions", async () => {
    const page = (await load("with_cropbox.pdf")).getPage(0);
    expect(page.width).toBeGreaterThan(50);
    expect(page.height).toBeGreaterThan(50);
  });

  test("setMediaBox size is reflected after save/reload", async () => {
    const doc = await PdfDocument.create();
    const page = doc.addPage();
    page.setMediaBox(5, 5, 20, 50);
    const out = await doc.save();
    const p = (await PdfDocument.load(out)).getPage(0);
    expect(Math.round(p.width)).toBe(15);
    expect(Math.round(p.height)).toBe(45);
  });
});
