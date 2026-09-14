import { createHash } from 'node:crypto';
import fs from 'node:fs';

/**
 * @param {string} filePath
 * @returns {string}
 */
export function sha256File(filePath) {
  return createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

/**
 * @typedef {{ stdout?: string, stderr?: string, status?: number | null }} CapturedRun
 * @typedef {{ field: 'stdout' | 'stderr' | 'status', expected: unknown, actual: unknown }} RunDifference
 */

/**
 * @param {CapturedRun} a
 * @param {CapturedRun} b
 * @returns {{ ok: boolean, differences: RunDifference[] }}
 */
export function compareRuns(a, b) {
  /** @type {RunDifference[]} */
  const differences = [];
  for (const field of /** @type {const} */ (['stdout', 'stderr', 'status'])) {
    if (a[field] !== b[field]) {
      differences.push({ field, expected: a[field], actual: b[field] });
    }
  }
  return { ok: differences.length === 0, differences };
}

/**
 * First diagnostic whose `rule` or `rule_id` equals `ruleId`, or ends with `/${ruleId}`.
 *
 * @param {unknown} doc
 * @param {string} ruleId
 * @returns {Record<string, unknown> | null}
 */
export function findDiagnosticRule(doc, ruleId) {
  if (doc === null || typeof doc !== 'object') {
    return null;
  }
  const diagnostics = /** @type {{ diagnostics?: unknown }} */ (doc).diagnostics;
  if (!Array.isArray(diagnostics)) {
    return null;
  }
  for (const item of diagnostics) {
    if (item === null || typeof item !== 'object') {
      continue;
    }
    const record = /** @type {Record<string, unknown>} */ (item);
    for (const candidate of [record.rule, record.rule_id]) {
      if (typeof candidate !== 'string') {
        continue;
      }
      if (candidate === ruleId || candidate.endsWith(`/${ruleId}`)) {
        return record;
      }
    }
  }
  return null;
}
