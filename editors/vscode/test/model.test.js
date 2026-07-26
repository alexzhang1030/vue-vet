'use strict';

const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const {
  modulesFromReport,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  buildTree,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
} = require('../lib/model');

const sample = JSON.parse(
  readFileSync(join(__dirname, 'fixtures/sample-reactivity.json'), 'utf8'),
);

describe('reactivity model', () => {
  it('parses modules_detail from a JSON report', () => {
    const modules = modulesFromReport(sample);
    assert.equal(modules.length, 1);
    assert.equal(modules[0].id, 'App.vue');
    assert.equal(modules[0].edge_details[0].label, 'v-if  →  error');
  });

  it('resolves a module by relative path or basename', () => {
    const modules = modulesFromReport(sample);
    assert.equal(moduleForFile(modules, 'App.vue')?.id, 'App.vue');
    assert.equal(moduleForFile(modules, 'src/App.vue')?.id, 'App.vue');
  });

  it('builds decoration and hover plans from structured spans', () => {
    const modules = modulesFromReport(sample);
    const module = modules[0];
    const plan = decorationPlan(module, null);
    assert.ok(plan.some((item) => item.role === 'binding' && item.span.offset === 10));
    assert.ok(plan.some((item) => item.role === 'edge' && item.span.offset === 40));

    const selected = decorationPlan(module, {
      kind: 'edge',
      key: 'edge:template:if@40->error@40',
    });
    assert.ok(selected.some((item) => item.role === 'selection'));

    const hover = hoverAtOffset(module, 42);
    assert.equal(hover?.label, 'v-if  →  error');
  });

  it('groups inbound edges under binding targets in the tree', () => {
    const tree = buildTree(modulesFromReport(sample));
    assert.equal(tree[0].label, 'App.vue');
    const inbound = tree[0].children.find((child) => child.label.startsWith('inbound graph'));
    assert.ok(inbound);
    assert.equal(inbound.children[0].label, '● error');
    assert.equal(inbound.children[0].children[0].kind, 'edge');
  });

  it('maps UTF-8 byte offsets through multi-byte prefixes for VS Code', () => {
    // "测" is 3 UTF-8 bytes / 1 UTF-16 unit; without conversion highlights shift right.
    const text = '测backend';
    const byteOffset = new TextEncoder().encode('测').length; // start of "backend"
    assert.equal(utf8OffsetToUtf16(text, byteOffset), 1);
    assert.equal(text.slice(utf8OffsetToUtf16(text, byteOffset)), 'backend');
    assert.equal(utf16OffsetToUtf8(text, 1), byteOffset);
    // ASCII stays identity.
    assert.equal(utf8OffsetToUtf16('backend', 3), 3);
  });
});
