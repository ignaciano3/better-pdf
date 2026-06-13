// Compile-time assertions for the FormBuilder typed API. This file has no
// runtime tests — it is checked by `bun run typecheck` (tsconfig includes
// "tests") and is intentionally NOT named `*.test.ts`. Each `@ts-expect-error`
// line MUST fail to compile; if it ever compiles, typecheck fails.
import { FormBuilder } from "../../src/generate/form-builder.js";

// Build a typed form schema via the builder.
declare const defs: ConstructorParameters<typeof FormBuilder>[0];
declare const names: Set<string>;

const fb = new FormBuilder(defs, names)
  .addTextField("myText", { page: 0, x: 0, y: 0, width: 100, height: 20 })
  .addDropdown("myDrop", { page: 0, x: 0, y: 0, width: 100, height: 20, options: ["AR", "BR"] as const })
  .addRadioGroup("myRadio", {
    options: [
      { value: "Yes", page: 0, x: 0, y: 0, size: 12 },
      { value: "No", page: 0, x: 0, y: 0, size: 12 },
    ] as const,
  });

// Positive — getFieldNames includes declared fields.
const names1: string[] = fb.getFieldNames();
// The type should be the union of declared names; check specific values compile.
const _checkText: "myText" | "myDrop" | "myRadio" = fb.getFieldNames()[0]!;

// Positive — dropdown selected accepts a declared option.
new FormBuilder(defs, names).addDropdown("c", {
  page: 0, x: 0, y: 0, width: 100, height: 20,
  options: ["AR", "BR"] as const,
  selected: "AR",          // must compile
});

// Negative — dropdown selected must be one of the declared options.
new FormBuilder(defs, names).addDropdown("c2", {
  page: 0, x: 0, y: 0, width: 100, height: 20,
  options: ["AR", "BR"] as const,
  // @ts-expect-error "ZZ" is not in options
  selected: "ZZ",
});

// Positive — radio selected accepts a declared value.
new FormBuilder(defs, names).addRadioGroup("r", {
  selected: "Yes",
  options: [
    { value: "Yes", page: 0, x: 0, y: 0, size: 12 },
    { value: "No", page: 0, x: 0, y: 0, size: 12 },
  ] as const,
});

// Negative — radio selected value must be a declared option value.
new FormBuilder(defs, names).addRadioGroup("r2", {
  // @ts-expect-error "Maybe" is not in options
  selected: "Maybe",
  options: [
    { value: "Yes", page: 0, x: 0, y: 0, size: 12 },
    { value: "No", page: 0, x: 0, y: 0, size: 12 },
  ] as const,
});

// Suppress "unused" warnings.
void names1;
void _checkText;
