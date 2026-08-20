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
 *   property?: string,
 *   to_path?: string,
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
 *   label: string,
 *   summary?: string
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
 * @typedef {{
 *   peer: string,
 *   kind: string,
 *   specifier: string,
 *   span: SpanRef
 * }} ComponentNavLink
 * @typedef {{
 *   id: string,
 *   uses: ComponentNavLink[],
 *   used_by: ComponentNavLink[]
 * }} ComponentNavModule
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
 * Structural component uses / used_by from the project graph (not prop dataflow).
 * @param {unknown} report
 * @returns {ComponentNavModule[]}
 */
function componentNavFromReport(report) {
  if (!report || typeof report !== 'object') {
    return [];
  }
  const modules = report.component_nav?.modules;
  if (!Array.isArray(modules)) {
    return [];
  }
  return modules.filter((module) => module && typeof module.id === 'string');
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
 * Tightest tracking scope covering a UTF-8 byte offset (same rule as
 * `scope_covering_span` / `--explain-scope @offset` covering fallback).
 * @param {ModuleDetail | undefined} module
 * @param {number} offset
 * @returns {ScopeDetail | null}
 */
function scopeAtOffset(module, offset) {
  if (!module || typeof offset !== 'number') {
    return null;
  }
  /** @type {ScopeDetail | null} */
  let best = null;
  for (const scope of module.scope_details || []) {
    if (!isSpan(scope.span)) continue;
    if (offset < scope.span.offset || offset >= scope.span.offset + scope.span.length) continue;
    if (
      !best ||
      scope.span.length < best.span.length ||
      (scope.span.length === best.span.length && scope.span.offset < best.span.offset)
    ) {
      best = scope;
    }
  }
  return best;
}

/**
 * Markdown for a CLI `--explain-scope` payload (one object or an array).
 * @param {unknown} payload
 * @returns {string}
 */
function markdownFromScopeExplain(payload) {
  const explains = Array.isArray(payload) ? payload : payload ? [payload] : [];
  if (explains.length === 0) {
    return '_No tracking scope matched._';
  }
  return explains.map(formatOneScopeExplain).join('\n\n---\n\n');
}

/**
 * @param {any} explain
 */
function formatOneScopeExplain(explain) {
  if (!explain || typeof explain !== 'object') {
    return '_Invalid scope explain._';
  }
  const who = explain.binding || explain.callee || 'scope';
  const lines = [
    `## ${who}`,
    '',
    `_${explain.kind || 'scope'}_ · \`${explain.module_id || ''}\``,
    '',
    explain.summary || '_No summary._',
  ];
  const tracks = Array.isArray(explain.tracks) ? explain.tracks : [];
  if (tracks.length) {
    lines.push('', '**Tracks**');
    for (const dep of tracks) {
      lines.push(`- \`${dep.path || dep.binding}\` — ${dep.reason_label || dep.reason || ''}`);
    }
  }
  const skipped = Array.isArray(explain.does_not_track) ? explain.does_not_track : [];
  if (skipped.length) {
    lines.push('', '**Does not track**');
    for (const dep of skipped) {
      lines.push(`- \`${dep.path || dep.binding}\` — ${dep.reason_label || dep.reason || ''}`);
    }
  }
  const uncertain = Array.isArray(explain.uncertain) ? explain.uncertain : [];
  if (uncertain.length) {
    lines.push('', `**Uncertain:** ${uncertain.map((name) => `\`${name}\``).join(', ')}`);
  }
  return lines.join('\n');
}

/**
 * Tree nodes shaped for the VS Code TreeDataProvider.
 * @param {ModuleDetail[]} modules
 * @param {ComponentNavModule[]} [componentNav]
 */
function buildTree(modules, componentNav = []) {
  /** @type {Map<string, ModuleDetail & { component_uses?: ComponentNavLink[], component_used_by?: ComponentNavLink[] }>} */
  const byId = new Map();
  for (const module of modules) {
    byId.set(normalizePath(module.id), { ...module });
  }
  for (const nav of componentNav) {
    const id = normalizePath(nav.id);
    const existing = byId.get(id);
    if (existing) {
      existing.component_uses = nav.uses || [];
      existing.component_used_by = nav.used_by || [];
    } else {
      byId.set(id, {
        id,
        bindings: [],
        scopes: [],
        edges: [],
        template_reads: [],
        component_uses: nav.uses || [],
        component_used_by: nav.used_by || [],
      });
    }
  }

  const sorted = [...byId.values()].sort((left, right) => left.id.localeCompare(right.id));
  return sorted.map((module) => {
    const weight =
      (module.binding_details?.length || module.bindings?.length || 0) +
      (module.scope_details?.length || module.scopes?.length || 0) +
      (module.edge_details?.length || module.edges?.length || 0) +
      (module.template_details?.length || module.template_reads?.length || 0) +
      (module.component_uses?.length || 0) +
      (module.component_used_by?.length || 0);

    /** @type {object[]} */
    const children = [];

    const bindings = module.binding_details || [];
    if (bindings.length) {
      children.push({
        kind: 'group',
        label: `bindings (${bindings.length})`,
        children: bindings.flatMap((binding) => {
          const nodes = [
            {
              kind: 'binding',
              moduleId: module.id,
              bindingName: binding.name,
              label: binding.label || binding.name,
              key: `binding:${binding.name}@${binding.span.offset}`,
              span: binding.span,
              description: binding.kind,
            },
          ];
          if (!isReactiveBagKind(binding.kind)) {
            return nodes;
          }
          for (const property of propertiesForBag(module, binding.name)) {
            nodes.push({
              kind: 'binding',
              moduleId: module.id,
              bindingName: `${binding.name}.${property}`,
              label: `${binding.name}.${property}`,
              key: `binding:${binding.name}.${property}@${binding.span.offset}`,
              span: binding.span,
              description: `${binding.kind} · .${property}`,
            });
          }
          return nodes;
        }),
      });
    }

    const uses = module.component_uses || [];
    if (uses.length) {
      children.push({
        kind: 'group',
        label: `components uses (${uses.length})`,
        description: 'structural · not prop dataflow',
        children: uses.map((link) => ({
          kind: 'component',
          moduleId: module.id,
          peer: link.peer,
          label: `<${link.specifier}> → ${link.peer}`,
          key: `component:uses:${module.id}->${link.peer}@${link.span?.offset ?? 0}`,
          span: link.span,
          description: link.kind,
        })),
      });
    }
    const usedBy = module.component_used_by || [];
    if (usedBy.length) {
      children.push({
        kind: 'group',
        label: `components used by (${usedBy.length})`,
        description: 'structural · not prop dataflow',
        children: usedBy.map((link) => ({
          kind: 'component',
          // Evidence span lives on the parent template tag (peer file).
          moduleId: link.peer,
          peer: link.peer,
          label: `${link.peer} → <${link.specifier}>`,
          key: `component:used_by:${link.peer}->${module.id}@${link.span?.offset ?? 0}`,
          span: link.span,
          description: link.kind,
        })),
      });
    }

    const edgesByTarget = new Map();
    for (const edge of module.edge_details || []) {
      const target = edge.to_path || toPath(edge.to, edge.property);
      const list = edgesByTarget.get(target) || [];
      list.push(edge);
      edgesByTarget.set(target, list);
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
 * @param {string | undefined} kind
 */
function isReactiveBagKind(kind) {
  return kind === 'reactive' || kind === 'shallow_reactive';
}

/**
 * @param {string} to
 * @param {string | undefined} property
 */
function toPath(to, property) {
  if (property) return `${to}.${property}`;
  return to;
}

/**
 * @param {ModuleDetail | undefined} module
 * @param {string} bag
 * @returns {string[]}
 */
function propertiesForBag(module, bag) {
  /** @type {Set<string>} */
  const properties = new Set();
  const prefix = `${bag}.`;
  for (const edge of module?.edge_details || []) {
    const path = edge.to_path || toPath(edge.to, edge.property);
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const property = rest.split('.')[0];
    if (property) properties.add(property);
  }
  return [...properties].sort((left, right) => left.localeCompare(right));
}

/**
 * @param {string} target `props` or `props.count`
 * @returns {{ binding: string, property: string | null }}
 */
function splitInspectTarget(target) {
  const dot = target.indexOf('.');
  if (dot === -1) {
    return { binding: target, property: null };
  }
  return { binding: target.slice(0, dot), property: target.slice(dot + 1) || null };
}

/**
 * @param {import('./model').EdgeDetail | { to: string, property?: string, to_path?: string }} edge
 * @param {string} binding
 * @param {string | null} property
 */
function edgeToMatches(edge, binding, property) {
  const path = edge.to_path || toPath(edge.to, edge.property);
  if (property) {
    return path === `${binding}.${property}`;
  }
  return path === binding || path.startsWith(`${binding}.`);
}

/**
 * Who reads / tracks a binding (inbound).
 * @param {ModuleDetail | undefined} module
 * @param {string} bindingName bare binding or `props.count`
 */
function inboundFor(module, bindingName) {
  /** @type {{ label: string, kind: string, span?: SpanRef, toSpan?: SpanRef, key: string }[]} */
  const items = [];
  if (!module || !bindingName) return items;
  const { binding, property } = splitInspectTarget(bindingName);
  for (const edge of module.edge_details || []) {
    if (!edgeToMatches(edge, binding, property)) continue;
    const path = edge.to_path || toPath(edge.to, edge.property);
    items.push({
      label: edge.label || `${edge.from} → ${path}`,
      kind: `reader · ${edge.kind}`,
      span: edge.span,
      toSpan: edge.to_span,
      key: `edge:${edge.from}->${path}@${edge.span?.offset ?? 0}`,
    });
  }
  // Template joins name the bare binding; include only for bag-level inspect.
  if (!property) {
    for (const read of module.template_details || []) {
      if (read.binding !== binding) continue;
      items.push({
        label: read.label || `${read.surface} reads ${read.binding}`,
        kind: `template · ${read.surface}`,
        span: read.span,
        key: `template:${read.binding}@${read.surface}@${read.span?.offset ?? 0}`,
      });
    }
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
  const { binding, property } = splitInspectTarget(bindingName);
  // Member picks are inbound-only.
  if (property) return items;
  for (const edge of module.edge_details || []) {
    if (!edgeFromIsBinding(edge.from, binding)) continue;
    const path = edge.to_path || toPath(edge.to, edge.property);
    items.push({
      label: edge.label || `${edge.from} → ${path}`,
      kind: `dependency · ${edge.kind}`,
      span: edge.span,
      toSpan: edge.to_span,
      key: `edge:${edge.from}->${path}@${edge.span?.offset ?? 0}`,
      to: path,
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

/**
 * @param {ComponentNavModule[] | undefined} componentNav
 * @param {string} moduleId
 * @param {'uses' | 'used_by'} direction
 */
function componentLinksFor(componentNav, moduleId, direction) {
  if (!Array.isArray(componentNav) || !moduleId) return [];
  const id = normalizePath(moduleId);
  const module = componentNav.find((item) => normalizePath(item.id) === id);
  if (!module) return [];
  const links = direction === 'uses' ? module.uses || [] : module.used_by || [];
  return [...links].sort((left, right) => {
    const peer = left.peer.localeCompare(right.peer);
    if (peer !== 0) return peer;
    return (left.span?.offset ?? 0) - (right.span?.offset ?? 0);
  });
}

module.exports = {
  modulesFromReport,
  componentNavFromReport,
  normalizePath,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  scopeAtOffset,
  markdownFromScopeExplain,
  buildTree,
  isSpan,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
  inboundFor,
  outboundFor,
  edgeFromIsBinding,
  bindingAtOffset,
  isReactiveBagKind,
  propertiesForBag,
  splitInspectTarget,
  toPath,
  componentLinksFor,
};
