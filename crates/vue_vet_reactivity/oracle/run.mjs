/**
 * One entry for the Node evidence lanes.
 *
 * Discovers `*.mjs` here except this file and `harness.mjs`.
 * Each lane file keeps its own premise.
 */
import { readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const skip = new Set(["harness.mjs", "run.mjs"]);
const lanes = readdirSync(root)
  .filter((name) => name.endsWith(".mjs") && !skip.has(name))
  .sort();

let failed = 0;
for (const lane of lanes) {
  const result = spawnSync(process.execPath, [path.join(root, lane)], {
    cwd: root,
    stdio: "inherit",
  });
  if ((result.status ?? 1) !== 0) {
    failed += 1;
    console.log(`oracle: fail ${lane}`);
    process.exit(result.status ?? 1);
  }
  console.log(`oracle: ok ${lane}`);
}
console.log(`oracle: ${lanes.length} lanes, ${failed} failed`);
