import assert from "node:assert/strict";
import test from "node:test";
import { randomId } from "./randomId.ts";

test("UUID generation does not require secure-context randomUUID", () => {
  const id = randomId({ getRandomValues: <T extends ArrayBufferView | null>(array: T): T => {
    (array as Uint8Array).fill(255);
    return array;
  } });
  assert.equal(id, "ffffffff-ffff-4fff-bfff-ffffffffffff");
});
