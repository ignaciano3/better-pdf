#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { PdfDocument } from "../index.js";
import { generateFormTypes } from "../typegen.js";

function usage(): string {
  return [
    "Usage: better-pdf-generate-types <input.pdf> [output.ts] [--name TypeName]",
    "",
    "Examples:",
    "  better-pdf-generate-types form.pdf src/form-types.ts",
    "  better-pdf-generate-types form.pdf --name EnrollmentForm > src/form-types.ts",
  ].join("\n");
}

function readName(args: string[]): string | undefined {
  const index = args.indexOf("--name");
  if (index === -1) return undefined;
  const value = args[index + 1];
  if (!value) throw new Error("--name requires a TypeScript identifier");
  args.splice(index, 2);
  return value;
}

export async function runGenerateTypesCli(args: string[]): Promise<void> {
  const mutableArgs = [...args];
  if (mutableArgs.includes("--help") || mutableArgs.includes("-h")) {
    console.log(usage());
    return;
  }

  const typeName = readName(mutableArgs);
  const [inputPath, outputPath, extra] = mutableArgs;
  if (!inputPath || extra) {
    throw new Error(usage());
  }

  const bytes = new Uint8Array(await readFile(inputPath));
  const doc = await PdfDocument.load(bytes);
  const source = generateFormTypes(doc.getForm().getFields(), { typeName });

  if (outputPath) {
    await writeFile(outputPath, source);
    return;
  }

  console.log(source);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runGenerateTypesCli(process.argv.slice(2)).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
