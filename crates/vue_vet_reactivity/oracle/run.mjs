/**
 * One entry for the Node evidence lanes.
 *
 * `just oracle-all` installs dependencies once, then runs this file.
 * Each lane keeps its own script. This runner only orders them.
 */
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));

const lanes = [
  "self-trigger-runs.mjs",
  "lifetime-runs.mjs",
  "lifetime-ownership-runs.mjs",
  "stale-settlement-runs.mjs",
  "source-contracts.mjs",
  "watch-api.mjs",
  "watch-callback-contracts.mjs",
  "value-contracts.mjs",
  "template-ref-demand.mjs",
  "derivation-practice.mjs",
  "scheduling-practice.mjs",
  "lost-notification-runs.mjs",
  "cleanup-identity-runs.mjs",
  "custom-ref-notification-runs.mjs",
  "cached-result-contracts.mjs",
  "computed-identity.mjs",
  "private-receiver.mjs",
  "until-demand.mjs",
  "injection-demand-contracts.mjs",
  "vueuse-demand.mjs",
  "filter-settlement-contracts.mjs",
  "snapshot-demand.mjs",
  "model-demand.mjs",
];

for (const lane of lanes) {
  const result = spawnSync(process.execPath, [path.join(root, lane)], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
