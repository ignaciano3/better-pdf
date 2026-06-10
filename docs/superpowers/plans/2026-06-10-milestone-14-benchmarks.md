# Milestone 14 — Benchmarks vs pdf-lib

**Status:** ✅ Implemented and merged.

**Goal:** Quantify better-pdf's performance against `pdf-lib` on equivalent
end-to-end operations, and professionalize the README.

## What shipped

- **`bench/bench.ts`** (`bun run bench`) — compares better-pdf vs pdf-lib over
  three scenarios on the bundled FICHA fixture: load + read fields, fill 10 text
  fields + save, and flatten all + save. Warmup pass then `ITER` iterations
  (env `BENCH_ITER`, default 50); prints a Markdown results table.
- `pdf-lib` added as a **devDependency** (benchmark only; zero runtime deps).
- **README professionalized** — Benchmarks section with the results table;
  removed the user's scratch `## New` section from `PLAN.md`; fixed the status
  line (typed forms are implemented, not "future").

## Results (indicative, bundled fixture, 50 iters after warmup)

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + read fields | ~0.49 ms | ~0.92 ms | ~1.9× |
| fill 10 text fields + save | ~1.05 ms | ~5.41 ms | ~5.1× |
| flatten all + save | ~1.01 ms | ~5.12 ms | ~5.1× |

Absolute timings vary by machine; the Rust/WASM core plus append-only
incremental saves drive the gap.

## Files

- Create `bench/bench.ts`. Modify `package.json` (`bench` script, pdf-lib
  devDep), `README.md`, `PLAN.md`.
