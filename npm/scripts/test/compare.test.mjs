import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, it } from 'node:test';
import { compareRuns, findDiagnosticRule, sha256File } from '../lib/compare.mjs';

describe('sha256File', () => {
  it('hashes file bytes', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vue-vet-compare-'));
    try {
      const file = path.join(dir, 'payload.bin');
      const contents = Buffer.from('hello-vue-vet');
      fs.writeFileSync(file, contents);
      assert.equal(
        sha256File(file),
        createHash('sha256').update(contents).digest('hex'),
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('differs when contents differ', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vue-vet-compare-'));
    try {
      const a = path.join(dir, 'a.bin');
      const b = path.join(dir, 'b.bin');
      fs.writeFileSync(a, 'aaa');
      fs.writeFileSync(b, 'bbb');
      assert.notEqual(sha256File(a), sha256File(b));
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('compareRuns', () => {
  it('returns ok when stdout, stderr, and status match', () => {
    const run = { stdout: 'out\n', stderr: '', status: 0 };
    const result = compareRuns(run, { stdout: 'out\n', stderr: '', status: 0 });
    assert.equal(result.ok, true);
    assert.deepEqual(result.differences, []);
  });

  it('lists each differing field', () => {
    const result = compareRuns(
      { stdout: 'a', stderr: '', status: 0 },
      { stdout: 'b', stderr: 'err', status: 1 },
    );
    assert.equal(result.ok, false);
    assert.deepEqual(result.differences, [
      { field: 'stdout', expected: 'a', actual: 'b' },
      { field: 'stderr', expected: '', actual: 'err' },
      { field: 'status', expected: 0, actual: 1 },
    ]);
  });

  it('treats null status as distinct from zero', () => {
    const result = compareRuns(
      { stdout: '', stderr: '', status: 0 },
      { stdout: '', stderr: '', status: null },
    );
    assert.equal(result.ok, false);
    assert.equal(result.differences.length, 1);
    assert.equal(result.differences[0].field, 'status');
  });
});

describe('findDiagnosticRule', () => {
  it('matches rule_id exactly or by trailing segment', () => {
    const doc = {
      diagnostics: [{ rule_id: 'vue-vet/security/no-v-html', file: 'App.vue' }],
    };
    const exact = findDiagnosticRule(doc, 'vue-vet/security/no-v-html');
    const short = findDiagnosticRule(doc, 'no-v-html');
    assert.equal(exact?.file, 'App.vue');
    assert.equal(short?.rule_id, 'vue-vet/security/no-v-html');
  });

  it('matches the rule field', () => {
    const doc = { diagnostics: [{ rule: 'no-v-html' }] };
    assert.equal(findDiagnosticRule(doc, 'no-v-html')?.rule, 'no-v-html');
  });

  it('returns null when the document or rule is absent', () => {
    assert.equal(findDiagnosticRule(null, 'no-v-html'), null);
    assert.equal(findDiagnosticRule({}, 'no-v-html'), null);
    assert.equal(findDiagnosticRule({ diagnostics: [] }, 'no-v-html'), null);
    assert.equal(
      findDiagnosticRule({ diagnostics: [{ rule_id: 'other' }] }, 'no-v-html'),
      null,
    );
  });
});
