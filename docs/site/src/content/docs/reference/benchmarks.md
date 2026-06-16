---
title: Benchmarks
description: better-pdf vs pdf-lib on end-to-end mutation and generation workloads.
---

`better-pdf` is consistently faster than `pdf-lib` on end-to-end mutation
workloads, thanks to its Rust/WebAssembly core and append-only incremental saves.
Indicative results from `bun run bench` on the bundled fixture corpus (50
iterations after warmup).

The **fill** and **flatten** rows are the like-for-like comparison. The
*load + save unchanged* rows compare better-pdf's no-op incremental round-trip
(it returns the original bytes) against pdf-lib's full parse + re-serialize —
they showcase the architectural difference, not parser speed.

## Small mixed form

`Form.-D.P.-2.4.1-Ficha-personal.pdf` — 57 KB, 30 fields: text, radio, dropdown.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.02 ms | 1.29 ms | 58.4× |
| load + read fields | 0.48 ms | 0.79 ms | 1.7× |
| fill 24 text fields + save | 1.10 ms | 5.86 ms | 5.3× |
| fill 2 choice fields + save | 0.80 ms | 4.57 ms | 5.7× |
| flatten all + save | 0.89 ms | 4.83 ms | 5.5× |

## Medium dense form

`Modulo-de-Diabetes.pdf` — 259 KB, 109 fields: text, radio, checkbox, dropdown, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.07 ms | 13.89 ms | 186.4× |
| load + read fields | 1.70 ms | 5.57 ms | 3.3× |
| fill 24 text fields + save | 3.87 ms | 26.43 ms | 6.8× |
| fill 19 choice fields + save | 3.77 ms | 27.03 ms | 7.2× |
| stamp 2 signature images + save | 8.31 ms | n/a | n/a |
| stamp first signature + flatten it | 7.49 ms | n/a | n/a |
| flatten all + save | 4.68 ms | error | n/a |

## Large signature form

`Convenio-OSFATUN-Discapacidad-2022.pdf` — 735 KB, 22 fields: text, signature.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| load + save unchanged | 0.24 ms | 1.33 ms | 5.5× |
| load + read fields | 0.36 ms | 0.78 ms | 2.1× |
| fill 20 text fields + save | 1.02 ms | 4.36 ms | 4.3× |
| stamp 2 signature images + save | 5.94 ms | n/a | n/a |
| stamp first signature + flatten it | 3.81 ms | n/a | n/a |
| flatten all + save | 0.96 ms | error | n/a |

## PDF generation

Building or stamping documents from scratch (no fixture). The `create + draw`
rows compare against `pdf-lib`'s equivalent generation API; vector shapes have no
direct `pdf-lib` one-liner equivalent.

| Scenario | better-pdf | pdf-lib | speedup |
| --- | ---: | ---: | ---: |
| create + draw text | 0.15 ms | 1.25 ms | 8.2× |
| stamp text on existing | 1.10 ms | 2.16 ms | 2.0× |
| create + draw image | 0.07 ms | 0.50 ms | 7.3× |
| create + vector shapes | 0.09 ms | n/a | n/a |

In the `error` rows, `pdf-lib` threw `Unexpected N type: undefined` while
flattening real-world fixtures. Absolute timings vary by machine; reproduce them
on yours with `bun run bench` (set `BENCH_ITER` to change the iteration count).
