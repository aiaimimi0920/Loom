import test from "node:test";
import assert from "node:assert/strict";
import { parseTransferStatus } from "./cloudDownloadControl.ts";

test("download progress binds task and rejects invalid counters", () => {
  const status = { taskId: "task", control: 2, total: 100, completed: 40, speed: 10 };
  assert.deepEqual(parseTransferStatus(status, "task"), status);
  for (const invalid of [{ ...status, taskId: "other" }, { ...status, completed: -1 },
    { ...status, total: 9 * 1024 * 1024 }, { ...status, control: 3 }, { ...status, speed: NaN }]) {
    assert.throws(() => parseTransferStatus(invalid, "task"), /asset_library_response_invalid/);
  }
});
