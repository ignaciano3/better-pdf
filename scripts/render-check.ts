// Render regression check: pdf.js renders the FICHA fixture in Chromium and we
// count dark pixels inside the filled field's /Rect. The filled and flattened
// outputs must both gain ink over the original. Run with `bun run test:render`
// (needs `bunx playwright install chromium`; does NOT need the built dist).
import { chromium } from "playwright";
import { readFileSync, existsSync } from "node:fs";
import { join, extname } from "node:path";
import { PdfDocument } from "../src/index.ts";

const ROOT = join(import.meta.dir, "..");
const FIXTURE = join(ROOT, "tests/fixtures/Discapacidad/Form.-D.P.-2.4.1-Ficha-personal.pdf");
const FIELD = "beneficiario.apellidos_nombres";
const TEXT = "WWWWWWWWWW";
const MIN_NEW_DARK_PIXELS = 30;

const original = new Uint8Array(readFileSync(FIXTURE));

async function build(flatten: boolean): Promise<Uint8Array> {
  const doc = await PdfDocument.load(original);
  const form = doc.getForm();
  form.getTextField(FIELD).setText(TEXT);
  if (flatten) form.flattenField(FIELD);
  return doc.save();
}

const widget = (await PdfDocument.load(original)).getForm().getField(FIELD)?.widgets[0];
if (!widget) throw new Error(`field ${FIELD} has no widget`);
const filled = await build(false);
const flattened = await build(true);

const page = /* html */ `<!doctype html><meta charset="utf-8"><body>
<script type="module">
  import * as pdfjs from "/node_modules/pdfjs-dist/build/pdf.min.mjs";
  pdfjs.GlobalWorkerOptions.workerSrc = "/node_modules/pdfjs-dist/build/pdf.worker.min.mjs";
  window.darkPixels = async (url, pageIndex, rect) => {
    const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
    const doc = await pdfjs.getDocument({ data: bytes }).promise;
    const pdfPage = await doc.getPage(pageIndex + 1);
    const viewport = pdfPage.getViewport({ scale: 2 });
    const canvas = document.createElement("canvas");
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    const ctx = canvas.getContext("2d");
    await pdfPage.render({ canvasContext: ctx, viewport }).promise;
    const [ax, ay] = viewport.convertToViewportPoint(rect[0], rect[1]);
    const [bx, by] = viewport.convertToViewportPoint(rect[2], rect[3]);
    const x = Math.max(0, Math.floor(Math.min(ax, bx)));
    const y = Math.max(0, Math.floor(Math.min(ay, by)));
    const w = Math.ceil(Math.abs(bx - ax));
    const h = Math.ceil(Math.abs(by - ay));
    const px = ctx.getImageData(x, y, w, h).data;
    let dark = 0;
    for (let i = 0; i < px.length; i += 4) {
      const lum = 0.299 * px[i] + 0.587 * px[i + 1] + 0.114 * px[i + 2];
      if (px[i + 3] > 0 && lum < 128) dark++;
    }
    return dark;
  };
</script></body>`;

const DOCS: Record<string, Uint8Array> = {
  "/original.pdf": original,
  "/filled.pdf": filled,
  "/flattened.pdf": flattened,
};

const TYPES: Record<string, string> = {
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".pdf": "application/pdf",
  ".html": "text/html",
};

const server = Bun.serve({
  port: 0,
  fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === "/") return new Response(page, { headers: { "content-type": "text/html" } });
    const doc = DOCS[path];
    if (doc) return new Response(Buffer.from(doc), { headers: { "content-type": "application/pdf" } });
    const file = join(ROOT, path);
    if (!file.startsWith(ROOT) || !existsSync(file)) {
      return new Response("not found", { status: 404 });
    }
    return new Response(readFileSync(file), {
      headers: { "content-type": TYPES[extname(file)] ?? "application/octet-stream" },
    });
  },
});

const browser = await chromium.launch();
let failed = false;
try {
  const tab = await (await browser.newContext()).newPage();
  tab.on("pageerror", (e) => console.error("[pageerror]", e.message));
  await tab.goto(`http://localhost:${server.port}`, { waitUntil: "load" });

  const count = (url: string) =>
    tab.evaluate(
      `window.darkPixels("${url}", ${widget.page}, [${widget.rect.join(",")}])`,
    ) as Promise<number>;

  const base = await count("/original.pdf");
  const fill = await count("/filled.pdf");
  const flat = await count("/flattened.pdf");
  console.log(`dark pixels — original: ${base}, filled: ${fill}, flattened: ${flat}`);

  if (fill < base + MIN_NEW_DARK_PIXELS) throw new Error("filled field rendered no visible text");
  if (flat < base + MIN_NEW_DARK_PIXELS) throw new Error("flattened field lost its text");
} catch (err) {
  failed = true;
  console.error("render check failed:", err);
} finally {
  await browser.close();
  server.stop(true);
}
process.exit(failed ? 1 : 0);
