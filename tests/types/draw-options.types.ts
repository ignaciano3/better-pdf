// Compile-time assertions for Draw*Options exports. This file has no runtime
// tests — it is checked by `bun run typecheck` (tsconfig includes "tests") and
// is intentionally NOT named `*.test.ts`, so `bun test` never runs it.
import type { DrawLinkOptions, DrawSvgPathOptions, DrawPolygonOptions } from "../../src/index.ts";

// These declarations confirm that all three types are importable and usable
// as type annotations. If any type is missing from the barrel, typecheck fails.
declare const _link: DrawLinkOptions;
declare const _svg: DrawSvgPathOptions;
declare const _poly: DrawPolygonOptions;

// Suppress unused-variable warnings by exporting.
export { _link, _svg, _poly };
