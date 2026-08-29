#!/usr/bin/env node
import { readFile, realpath, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { PdfDocument } from "../index.js";
import { generateFormTypes } from "../forms/typegen.js";

function usage(): string {
  return [
    "Usage: better-pdf-generate-types <input.pdf> [output.ts] [--name TypeName] [--password PW] [--include-values]",
    "",
    "Examples:",
    "  better-pdf-generate-types form.pdf src/form-types.ts",
    "  better-pdf-generate-types form.pdf --name EnrollmentForm > src/form-types.ts",
    "  better-pdf-generate-types secured.pdf --password s3cret > src/form-types.ts",
    "",
    "Encrypted PDFs need --password; pass an empty one (--password '') for",
    "owner-locked files that open without a user password.",
    "",
    "--include-values also emits each field's current value. Off by default so",
    "generating from a filled form never commits its answers; use it only on",
    "blank or reference forms.",
  ].join("\n");
}

/** Read a valueless `--flag`, removing it from `args`. */
function readFlag(args: string[], flag: string): boolean {
  const index = args.indexOf(flag);
  if (index === -1) return false;
  args.splice(index, 1);
  return true;
}

/**
 * Read `--flag value`, removing both from `args`. Returns `undefined` when the
 * flag is absent. An empty value is only accepted when `allowEmpty` is set —
 * `--password ""` is meaningful (owner-locked files), `--name ""` is not.
 */
function readOption(args: string[], flag: string, allowEmpty: boolean): string | undefined {
  const index = args.indexOf(flag);
  if (index === -1) return undefined;
  const value = args[index + 1];
  if (value === undefined || (!allowEmpty && value === "")) {
    throw new Error(`${flag} requires a value`);
  }
  args.splice(index, 2);
  return value;
}

export async function runGenerateTypesCli(args: string[]): Promise<void> {
  const mutableArgs = [...args];
  if (mutableArgs.includes("--help") || mutableArgs.includes("-h")) {
    console.log(usage());
    return;
  }

  const typeName = readOption(mutableArgs, "--name", false);
  const password = readOption(mutableArgs, "--password", true);
  const includeValues = readFlag(mutableArgs, "--include-values");
  const [inputPath, outputPath, extra] = mutableArgs;
  if (!inputPath || extra) {
    throw new Error(usage());
  }

  const bytes = new Uint8Array(await readFile(inputPath));
  const doc = await PdfDocument.load(bytes, password === undefined ? undefined : { password });
  const source = generateFormTypes(doc.getForm().getFields(), { typeName, includeValues });

  if (outputPath) {
    await writeFile(outputPath, source);
    return;
  }

  console.log(source);
}

async function isCliEntrypoint(): Promise<boolean> {
  if (!process.argv[1]) return false;

  const [modulePath, argvPath] = await Promise.all([
    realpath(fileURLToPath(import.meta.url)),
    realpath(process.argv[1]),
  ]);
  return modulePath === argvPath;
}

if (await isCliEntrypoint()) {
  runGenerateTypesCli(process.argv.slice(2)).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
