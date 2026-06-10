// Real headless-browser test: loads the `--target web` build in Chromium,
// instantiates the WASM via fetch, then exercises load -> read -> fill -> save
// entirely in the page. Run with `bun run test:browser` (NOT part of `bun test`,
// because it needs the built bundle plus a Playwright browser binary).
//
// Requires: `bun run build` (dist/ + pkg-web/) and `bunx playwright install chromium`.
import { chromium } from "playwright";
import { readFileSync, existsSync } from "node:fs";
import { join, extname } from "node:path";

const ROOT = join(import.meta.dir, "..");
const FIXTURE = "tests/fixtures/Discapacidad/Anexo-3-sssalud.pdf";

for (const required of ["dist/index.browser.js", "pkg-web/better_pdf_core_bg.wasm"]) {
  if (!existsSync(join(ROOT, required))) {
    throw new Error(`missing build artifact ${required}; run \`bun run build\` first`);
  }
}

const TYPES: Record<string, string> = {
  ".js": "text/javascript",
  ".wasm": "application/wasm",
  ".pdf": "application/pdf",
  ".html": "text/html",
};

const page = /* html */ `<!doctype html><meta charset="utf-8"><body>
<script type="module">
  import { PdfDocument } from "/dist/index.browser.js";
  window.run = async () => {
    const bytes = new Uint8Array(await (await fetch("/${FIXTURE}")).arrayBuffer());
    const doc = await PdfDocument.load(bytes);
    const form = doc.getForm();
    const fields = form.getFields();
    const text = fields.find((f) => f.type === "text");
    if (text) form.getTextField(text.name).setText("BROWSER OK");
    const out = await doc.save();
    const header = new TextDecoder().decode(out.slice(0, 5));
    return { count: fields.length, header, filled: text?.name ?? null };
  };
</script></body>`;

const server = Bun.serve({
  port: 0,
  fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === "/" || path === "/index.html") {
      return new Response(page, { headers: { "content-type": "text/html" } });
    }
    const file = join(ROOT, path);
    if (!file.startsWith(ROOT) || !existsSync(file)) {
      return new Response("not found", { status: 404 });
    }
    return new Response(readFileSync(file), {
      headers: { "content-type": TYPES[extname(file)] ?? "application/octet-stream" },
    });
  },
});

const base = `http://localhost:${server.port}`;
const browser = await chromium.launch();
let failed = false;
try {
  const ctx = await browser.newContext();
  const tab = await ctx.newPage();
  tab.on("console", (m) => m.type() === "error" && console.error("[page]", m.text()));
  tab.on("pageerror", (e) => console.error("[pageerror]", e.message));
  await tab.goto(base, { waitUntil: "load" });
  const result = (await tab.evaluate("window.run()")) as {
    count: number;
    header: string;
    filled: string | null;
  };

  if (result.count <= 0) throw new Error("no fields read in the browser");
  if (result.header !== "%PDF-") throw new Error(`saved bytes are not a PDF: ${result.header}`);
  if (!result.filled) throw new Error("fixture had no text field to fill");
  console.log(
    `browser OK: read ${result.count} fields, filled '${result.filled}', saved a valid PDF`,
  );
} catch (err) {
  failed = true;
  console.error("browser test failed:", err);
} finally {
  await browser.close();
  server.stop(true);
}

process.exit(failed ? 1 : 0);
