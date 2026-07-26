'use strict';

const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const {
  modulesFromReport,
  componentNavFromReport,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  buildTree,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
  inboundFor,
  outboundFor,
  bindingAtOffset,
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

  it('lists inbound readers and outbound dependencies for a binding', () => {
    const module = {
      id: 'App.vue',
      bindings: [],
      scopes: [],
      edges: [],
      template_reads: [],
      binding_details: [
        { name: 'count', kind: 'ref', span: { offset: 10, length: 5 }, label: 'count  (ref)' },
        {
          name: 'double',
          kind: 'computed',
          span: { offset: 20, length: 6 },
          label: 'double  (computed)',
        },
      ],
      edge_details: [
        {
          from: 'double',
          to: 'count',
          kind: 'computed',
          span: { offset: 30, length: 5 },
          to_span: { offset: 10, length: 5 },
          label: 'double  →  count',
          to_path: 'count',
        },
        {
          from: 'template:if@40',
          to: 'count',
          kind: 'template',
          span: { offset: 40, length: 2 },
          label: 'v-if  →  count',
          to_path: 'count',
        },
      ],
      template_details: [],
    };
    const readers = inboundFor(module, 'count');
    assert.equal(readers.length, 2);
    const deps = outboundFor(module, 'double');
    assert.equal(deps.length, 1);
    assert.equal(deps[0].to, 'count');
    assert.equal(bindingAtOffset(module, 12), 'count');
    assert.equal(bindingAtOffset(module, 22), 'double');
  });

  it('expands reactive bags into props.* tree nodes and filters inbound by property', () => {
    const module = {
      id: 'Child.vue',
      bindings: [],
      scopes: [],
      edges: [],
      template_reads: [],
      binding_details: [
        {
          name: 'props',
          kind: 'reactive',
          span: { offset: 4, length: 5 },
          label: 'props  (reactive)',
        },
        {
          name: 'label',
          kind: 'computed',
          span: { offset: 20, length: 5 },
          label: 'label  (computed)',
        },
      ],
      edge_details: [
        {
          from: 'label',
          to: 'props',
          property: 'count',
          to_path: 'props.count',
          kind: 'computed',
          span: { offset: 30, length: 5 },
          label: 'label  →  props.count',
        },
        {
          from: 'watch_sources:watch@40',
          to: 'props',
          property: 'mode',
          to_path: 'props.mode',
          kind: 'effect',
          span: { offset: 40, length: 4 },
          label: 'watch()  →  props.mode',
        },
      ],
      template_details: [
        {
          binding: 'props',
          surface: 'if',
          span: { offset: 50, length: 2 },
          label: 'v-if  reads  props',
        },
      ],
    };

    const tree = buildTree([module]);
    const bindingGroup = tree[0].children.find((child) => child.label.startsWith('bindings'));
    const names = bindingGroup.children.map((child) => child.bindingName);
    assert.deepEqual(names, ['props', 'props.count', 'props.mode', 'label']);

    const bagReaders = inboundFor(module, 'props');
    assert.equal(bagReaders.length, 3);
    const countReaders = inboundFor(module, 'props.count');
    assert.equal(countReaders.length, 1);
    assert.match(countReaders[0].label, /props\.count/);
    assert.equal(inboundFor(module, 'props.mode').length, 1);

    const deps = outboundFor(module, 'label');
    assert.equal(deps.length, 1);
    assert.equal(deps[0].to, 'props.count');
    assert.equal(outboundFor(module, 'props.count').length, 0);

    const inbound = tree[0].children.find((child) => child.label.startsWith('inbound graph'));
    const targets = inbound.children.map((child) => child.label);
    assert.ok(targets.includes('● props.count'));
    assert.ok(targets.includes('● props.mode'));
  });

  it('attaches structural component uses / used_by without inventing prop dataflow', () => {
    const report = {
      reactivity: {
        modules_detail: [
          {
            id: 'pages/index.vue',
            bindings: [],
            scopes: [],
            edges: [],
            template_reads: [],
          },
        ],
      },
      component_nav: {
        modules: [
          {
            id: 'pages/index.vue',
            uses: [
              {
                peer: 'components/Demo.vue',
                kind: 'auto_component',
                specifier: 'Demo',
                span: { offset: 12, length: 4 },
              },
            ],
            used_by: [],
          },
          {
            id: 'components/Demo.vue',
            uses: [],
            used_by: [
              {
                peer: 'pages/index.vue',
                kind: 'auto_component',
                specifier: 'Demo',
                span: { offset: 12, length: 4 },
              },
            ],
          },
        ],
      },
    };
    const nav = componentNavFromReport(report);
    assert.equal(nav.length, 2);
    const tree = buildTree(modulesFromReport(report), nav);
    const page = tree.find((node) => node.label === 'pages/index.vue');
    const demo = tree.find((node) => node.label === 'components/Demo.vue');
    assert.ok(page);
    assert.ok(demo);
    const uses = page.children.find((child) => child.label.startsWith('components uses'));
    assert.ok(uses);
    assert.equal(uses.children[0].kind, 'component');
    assert.match(uses.description, /not prop dataflow/);
    const usedBy = demo.children.find((child) => child.label.startsWith('components used by'));
    assert.ok(usedBy);
    // Evidence reveal opens the parent template file.
    assert.equal(usedBy.children[0].moduleId, 'pages/index.vue');
  });
});
