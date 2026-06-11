import { test, expect } from "bun:test";
import { FillQueue } from "../src/fields.ts";

test("FillQueue packs images into one blob with offsets", () => {
  const q = new FillQueue();
  q.push({ name: "a", value: "x" });
  q.push({ name: "b", image: new Uint8Array([1, 2, 3]) });
  q.push({ name: "c", image: new Uint8Array([4, 5]) });
  const { opsJson, images } = q.toPayload();
  expect(JSON.parse(opsJson)).toEqual([
    { name: "a", value: "x" },
    { name: "b", imageOffset: 0, imageLength: 3 },
    { name: "c", imageOffset: 3, imageLength: 2 },
  ]);
  expect([...images]).toEqual([1, 2, 3, 4, 5]);
});
