import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

const hook = ".githooks/pre-push.local";
const lane = "scripts/test-lane.sh";

test("fast pre-push shell regression covers the hook and lane", () => {
  const script = readFileSync("scripts/tests/pre-push-fast.sh", "utf8");
  assert.ok(script.includes(hook), "shell regression must exercise the real hook");
  assert.ok(script.includes(lane), "shell regression must exercise the real lane");

  const result = spawnSync("bash", ["scripts/tests/pre-push-fast.sh"], {
    cwd: process.cwd(),
    encoding: "utf8",
  });
  assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
});
