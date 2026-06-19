import { expect, test } from "bun:test";
import { wrapText } from "../src/generate/wrap-text.js";

// Deterministic measurer: 1 unit per character.
const len = (s: string): number => s.length;

test("greedy wraps words to fit maxWidth", () => {
  expect(wrapText("aaa bbb ccc", 7, len)).toBe("aaa bbb\nccc");
});

test("preserves explicit newlines as hard breaks", () => {
  expect(wrapText("aa\nbb cc", 3, len)).toBe("aa\nbb\ncc");
});

test("a single word wider than maxWidth overflows on its own line", () => {
  expect(wrapText("aaaaaa bb", 3, len)).toBe("aaaaaa\nbb");
});

test("text that already fits is returned unchanged", () => {
  expect(wrapText("hi there", 100, len)).toBe("hi there");
});

test("empty string returns empty string", () => {
  expect(wrapText("", 10, len)).toBe("");
});

test("collapses runs of spaces inside a paragraph to single spaces", () => {
  // words are split on spaces and rejoined with a single space
  expect(wrapText("aa   bb", 100, len)).toBe("aa bb");
});
