'use strict';

/**
 * Pure helpers for Vue Vet reactivity JSON. No VS Code imports — unit-tested
 * with node:test. The extension host only maps these to decorations / TreeView.
 */

/**
 * @typedef {{ offset: number, length: number }} SpanRef
 * @typedef {{
 *   name: string,
 *   kind: string,
 *   span: SpanRef,
 *   label: string
 * }} BindingDetail
 * @typedef {{
 *   from: string,
 *   to: string,
 *   to_id?: string,
 *   kind: string,
 *   span: SpanRef,
 *   to_span?: SpanRef,
 *   label: string
 * }} EdgeDetail
 * @typedef {{
 *   binding: string,
 *   surface: string,
 *   span: SpanRef,
 *   label: string
 * }} TemplateDetail
 * @typedef {{
 *   kind: string,
 *   callee: string,
 *   binding?: string,
 *   span: SpanRef,
 *   label: string
 * }} ScopeDetail
 * @typedef {{
 *   id: string,
 *   bindings: string[],
 *   scopes: string[],
 *   edges: string[],
 *   template_reads: string[],
 *   binding_details?: BindingDetail[],
 *   scope_details?: ScopeDetail[],
 *   edge_details?: EdgeDetail[],
 *   template_details?: TemplateDetail[]
 * }} ModuleDetail
 */

/**
 * @param {unknown} report
 * @returns {ModuleDetail[]}
 */
function modulesFromReport(report) {
  if (!report || typeof report !== 'object') {
    return [];
  }
  const detail = report.reactivity?.modules_detail;
  if (!Array.isArray(detail)) {
    return [];
  }
  return detail.filter((module) => module && typeof module.id === 'string');
}

/**
 * Normalize module ids and workspace-relative paths for comparison.
 * @param {string} value
 */
function normalizePath(value) {
  return value.replace(/\\/g, '/').replace(/^\.\//, '');
}

/**
 * Vue Vet spans are UTF-8 byte offsets into the original file. VS Code
 * `positionAt` / `offsetAt` use UTF-16 code units — never treat them as equal.
 *
 * @param {string} text
 * @param {number} byteOffset
 * @returns {number} UTF-16 offset
 */
function utf8OffsetToUtf16(text, byteOffset) {
  if (!Number.isFinite(byteOffset) || byteOffset <= 0) {
    return 0;
  }
  const encoder = new TextEncoder();
  let bytes = 0;
  let utf16 = 0;
  while (utf16 < text.length && bytes < byteOffset) {
    const codePoint = text.codePointAt(utf16);
    if (codePoint === undefined) {
      break;
    }
    const width = codePoint > 0xffff ? 2 : 1;
    const encoded = encoder.encode(String.fromCodePoint(codePoint));
    if (bytes + encoded.length > byteOffset) {
      break;
    }
    bytes += encoded.length;
    utf16 += width;
  }
  return utf16;
}

/**
 * @param {string} text
 * @param {number} utf16Offset
 * @returns {number} UTF-8 byte offset
 */
function utf16OffsetToUtf8(text, utf16Offset) {
  if (!Number.isFinite(utf16Offset) || utf16Offset <= 0) {
    return 0;
  }
  const clamped = Math.min(Math.max(0, Math.floor(utf16Offset)), text.length);
  return new TextEncoder().encode(text.slice(0, clamped)).length;
}

/**
 * @param {ModuleDetail[]} modules
 * @param {string} relativePath workspace-relative path of the open document
 * @returns {ModuleDetail | undefined}
 */
function moduleForFile(modules, relativePath) {
  const target = normalizePath(relativePath);
  const basename = target.split('/').pop();
  return (
    modules.find((module) => normalizePath(module.id) === target) ||
    modules.find((module) => normalizePath(module.id).endsWith(`/${target}`)) ||
    modules.find((module) => normalizePath(module.id).endsWith(`/${basename}`)) ||
    modules.find((module) => normalizePath(module.id) === basename)
  );
}

/**
 * @param {SpanRef | undefined} span
 * @returns {span is SpanRef}
 */
function isSpan(span) {
  return (
    !!span &&
    typeof span.offset === 'number' &&
    typeof span.length === 'number' &&
    span.offset >= 0 &&
    span.length > 0
  );
}

/**
 * Collect decoration targets for one module / optional selection.
 * @param {ModuleDetail | undefined} module
 * @param {{ kind: 'binding' | 'edge' | 'template', key: string } | null} selection
 */
function decorationPlan(module, selection = null) {
  /** @type {{ role: 'binding' | 'edge' | 'template' | 'selection', span: SpanRef, label: string, key: string }[]} */
  const items = [];
  if (!module) {
    return items;
  }

  for (const binding of module.binding_details || []) {
    if (!isSpan(binding.span)) continue;
    const key = `binding:${binding.name}@${binding.span.offset}`;
    const selected = selection?.kind === 'binding' && selection.key === key;
    items.push({
      role: selected ? 'selection' : 'binding',
      span: binding.span,
      label: binding.label || `${binding.name} (${binding.kind})`,
      key,
    });
  }

  for (const edge of module.edge_details || []) {
    if (!isSpan(edge.span)) continue;
    const key = `edge:${edge.from}->${edge.to}@${edge.span.offset}`;
    const selected = selection?.kind === 'edge' && selection.key === key;
    items.push({
      role: selected ? 'selection' : 'edge',
      span: edge.span,
      label: edge.label || `${edge.from} → ${edge.to}`,
      key,
    });
    if (selected && isSpan(edge.to_span)) {
      items.push({
        role: 'selection',
        span: edge.to_span,
        label: edge.label || edge.to,
        key: `${key}:to`,
      });
    }
  }

  for (const read of module.template_details || []) {
    if (!isSpan(read.span)) continue;
    const key = `template:${read.binding}@${read.surface}@${read.span.offset}`;
    const selected = selection?.kind === 'template' && selection.key === key;
    items.push({
      role: selected ? 'selection' : 'template',
      span: read.span,
      label: read.label || `${read.surface} reads ${read.binding}`,
      key,
    });
  }

  return items;
}

/**
 * Hover label for a byte offset in a module.
 * @param {ModuleDetail | undefined} module
 * @param {number} offset
 */
function hoverAtOffset(module, offset) {
  if (!module || typeof offset !== 'number') {
    return null;
  }
  /** @type {{ label: string, kind: string, distance: number } | null} */
  let best = null;
  const consider = (span, label, kind) => {
    if (!isSpan(span)) return;
    if (offset < span.offset || offset >= span.offset + span.length) return;
    const distance = offset - span.offset;
    if (!best || distance < best.distance) {
      best = { label, kind, distance };
    }
  };

  for (const binding of module.binding_details || []) {
    consider(binding.span, binding.label, `binding · ${binding.kind}`);
  }
  for (const edge of module.edge_details || []) {
    consider(edge.span, edge.label, `edge · ${edge.kind}`);
    consider(edge.to_span, edge.label, `edge target · ${edge.to}`);
  }
  for (const scope of module.scope_details || []) {
    consider(scope.span, scope.label, `scope · ${scope.kind}`);
  }
  for (const read of module.template_details || []) {
    consider(read.span, read.label, `template · ${read.surface}`);
  }
  return best;
}

/**
 * Tree nodes shaped for the VS Code TreeDataProvider.
 * @param {ModuleDetail[]} modules
 */
function buildTree(modules) {
  const sorted = [...modules].sort((left, right) => left.id.localeCompare(right.id));
  return sorted.map((module) => {
    const weight =
      (module.binding_details?.length || module.bindings?.length || 0) +
      (module.scope_details?.length || module.scopes?.length || 0) +
      (module.edge_details?.length || module.edges?.length || 0) +
      (module.template_details?.length || module.template_reads?.length || 0);

    /** @type {object[]} */
    const children = [];

    const bindings = module.binding_details || [];
    if (bindings.length) {
      children.push({
        kind: 'group',
        label: `bindings (${bindings.length})`,
        children: bindings.map((binding) => ({
          kind: 'binding',
          moduleId: module.id,
          bindingName: binding.name,
          label: binding.label || binding.name,
          key: `binding:${binding.name}@${binding.span.offset}`,
          span: binding.span,
          description: binding.kind,
        })),
      });
    }

    const edgesByTarget = new Map();
    for (const edge of module.edge_details || []) {
      const list = edgesByTarget.get(edge.to) || [];
      list.push(edge);
      edgesByTarget.set(edge.to, list);
    }
    if (edgesByTarget.size) {
      children.push({
        kind: 'group',
        label: `inbound graph (${edgesByTarget.size})`,
        children: [...edgesByTarget.entries()]
          .sort(([left], [right]) => left.localeCompare(right))
          .map(([target, edges]) => ({
            kind: 'group',
            label: `● ${target}`,
            children: edges.map((edge) => ({
              kind: 'edge',
              moduleId: module.id,
              label: edge.label || edge.from,
              key: `edge:${edge.from}->${edge.to}@${edge.span.offset}`,
              span: edge.span,
              toSpan: edge.to_span,
              description: edge.kind,
            })),
          })),
      });
    }

    const templates = module.template_details || [];
    if (templates.length) {
      children.push({
        kind: 'group',
        label: `template reads (${templates.length})`,
        children: templates.map((read) => ({
          kind: 'template',
          moduleId: module.id,
          label: read.label || `${read.surface} → ${read.binding}`,
          key: `template:${read.binding}@${read.surface}@${read.span.offset}`,
          span: read.span,
          description: read.surface,
        })),
      });
    }

    return {
      kind: 'module',
      moduleId: module.id,
      label: module.id,
      description: `${weight} facts`,
      children,
    };
  });
}

/**
 * Who reads / tracks a binding (inbound).
 * @param {ModuleDetail | undefined} module
 * @param {string} bindingName
 */
function inboundFor(module, bindingName) {
  /** @type {{ label: string, kind: string, span?: SpanRef, toSpan?: SpanRef, key: string }[]} */
  const items = [];
  if (!module || !bindingName) return items;
  for (const edge of module.edge_details || []) {
    if (edge.to !== bindingName) continue;
    items.push({
      label: edge.label || `${edge.from} → ${edge.to}`,
      kind: `reader · ${edge.kind}`,
      span: edge.span,
      toSpan: edge.to_span,
      key: `edge:${edge.from}->${edge.to}@${edge.span?.offset ?? 0}`,
    });
  }
  for (const read of module.template_details || []) {
    if (read.binding !== bindingName) continue;
    items.push({
      label: read.label || `${read.surface} reads ${read.binding}`,
      kind: `template · ${read.surface}`,
      span: read.span,
      key: `template:${read.binding}@${read.surface}@${read.span?.offset ?? 0}`,
    });
  }
  items.sort((left, right) => left.label.localeCompare(right.label));
  return items;
}

/**
 * What a computed / effect-as-from binding depends on (outbound).
 * @param {ModuleDetail | undefined} module
 * @param {string} bindingName
 */
function outboundFor(module, bindingName) {
  /** @type {{ label: string, kind: string, span?: SpanRef, toSpan?: SpanRef, key: string, to: string }[]} */
  const items = [];
  if (!module || !bindingName) return items;
  for (const edge of module.edge_details || []) {
    if (!edgeFromIsBinding(edge.from, bindingName)) continue;
    items.push({
      label: edge.label || `${edge.from} → ${edge.to}`,
      kind: `dependency · ${edge.kind}`,
      span: edge.span,
      toSpan: edge.to_span,
      key: `edge:${edge.from}->${edge.to}@${edge.span?.offset ?? 0}`,
      to: edge.to,
    });
  }
  items.sort((left, right) => left.label.localeCompare(right.label));
  return items;
}

/**
 * @param {string} from
 * @param {string} binding
 */
function edgeFromIsBinding(from, binding) {
  if (from === binding) return true;
  const head = from.includes('@') ? from.slice(0, from.lastIndexOf('@')) : from;
  return head === binding || head.endsWith(`:${binding}`);
}

/**
 * Resolve a binding name under a UTF-8 byte offset (editor cursor).
 * @param {ModuleDetail | undefined} module
 * @param {number} byteOffset
 */
function bindingAtOffset(module, byteOffset) {
  if (!module || typeof byteOffset !== 'number') return null;
  for (const binding of module.binding_details || []) {
    if (!isSpan(binding.span)) continue;
    if (byteOffset >= binding.span.offset && byteOffset < binding.span.offset + binding.span.length) {
      return binding.name;
    }
  }
  for (const edge of module.edge_details || []) {
    if (isSpan(edge.to_span) && byteOffset >= edge.to_span.offset && byteOffset < edge.to_span.offset + edge.to_span.length) {
      return edge.to;
    }
  }
  return null;
}

module.exports = {
  modulesFromReport,
  normalizePath,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  buildTree,
  isSpan,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
  inboundFor,
  outboundFor,
  edgeFromIsBinding,
  bindingAtOffset,
};
