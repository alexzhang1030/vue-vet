'use strict';

const path = require('node:path');
const vscode = require('vscode');
const { runReactivityScan, runExplainScope } = require('./lib/cli');
const {
  modulesFromReport,
  componentNavFromReport,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  scopeAtOffset,
  markdownFromScopeExplain,
  explainScopeQuery,
  normalizePath,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
  inboundFor,
  outboundFor,
  bindingAtOffset,
  componentLinksFor,
} = require('./lib/model');
const { ReactivityTreeProvider } = require('./lib/tree');

/** @type {vscode.TextEditorDecorationType} */
let bindingDecoration;
/** @type {vscode.TextEditorDecorationType} */
let edgeDecoration;
/** @type {vscode.TextEditorDecorationType} */
let selectionDecoration;

/** @type {import('./lib/model').ModuleDetail[]} */
let modules = [];
/** @type {import('./lib/model').ComponentNavModule[]} */
let componentNav = [];
/** @type {{ kind: 'binding' | 'edge' | 'template' | 'component', key: string, moduleId: string } | null} */
let selection = null;
/** @type {ReactivityTreeProvider} */
let treeProvider;
/** @type {vscode.StatusBarItem} */
let status;

/**
 * @param {vscode.ExtensionContext} context
 */
function activate(context) {
  bindingDecoration = vscode.window.createTextEditorDecorationType({
    backgroundColor: new vscode.ThemeColor('vueVet.bindingHighlight'),
    overviewRulerColor: new vscode.ThemeColor('vueVet.bindingHighlight'),
    overviewRulerLane: vscode.OverviewRulerLane.Center,
  });
  edgeDecoration = vscode.window.createTextEditorDecorationType({
    backgroundColor: new vscode.ThemeColor('vueVet.edgeHighlight'),
    overviewRulerColor: new vscode.ThemeColor('vueVet.edgeHighlight'),
    overviewRulerLane: vscode.OverviewRulerLane.Right,
  });
  selectionDecoration = vscode.window.createTextEditorDecorationType({
    backgroundColor: new vscode.ThemeColor('vueVet.selectionHighlight'),
    border: '1px solid',
    borderColor: new vscode.ThemeColor('vueVet.selectionHighlight'),
  });

  treeProvider = new ReactivityTreeProvider();
  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  status.command = 'vue-vet.showReactivity';
  status.text = '$(graph) Vue Vet';
  status.tooltip = 'Show Vue Vet reactivity graph';
  status.show();

  context.subscriptions.push(
    bindingDecoration,
    edgeDecoration,
    selectionDecoration,
    status,
    vscode.window.registerTreeDataProvider('vueVet.reactivity', treeProvider),
    vscode.commands.registerCommand('vue-vet.showReactivity', () => refreshReactivity(true)),
    vscode.commands.registerCommand('vue-vet.refreshReactivity', () => refreshReactivity(false)),
    vscode.commands.registerCommand('vue-vet.clearReactivity', clearHighlights),
    vscode.commands.registerCommand('vue-vet.revealTreeNode', revealTreeNode),
    vscode.commands.registerCommand('vue-vet.showReaders', (element) =>
      inspectBinding(element, 'readers'),
    ),
    vscode.commands.registerCommand('vue-vet.showDependencies', (element) =>
      inspectBinding(element, 'dependencies'),
    ),
    vscode.commands.registerCommand('vue-vet.showComponentsUsed', (element) =>
      inspectComponents(element, 'uses'),
    ),
    vscode.commands.registerCommand('vue-vet.showComponentUsers', (element) =>
      inspectComponents(element, 'used_by'),
    ),
    vscode.commands.registerCommand('vue-vet.explainScope', () => explainScopeAtCursor()),
    vscode.languages.registerHoverProvider(
      [
        { language: 'vue' },
        { language: 'typescript' },
        { language: 'javascript' },
        { language: 'typescriptreact' },
        { language: 'javascriptreact' },
      ],
      { provideHover },
    ),
    vscode.window.onDidChangeActiveTextEditor(() => applyDecorations()),
    vscode.workspace.onDidSaveTextDocument((document) => {
      if (!vscode.workspace.getConfiguration('vue-vet').get('refreshOnSave')) {
        return;
      }
      if (!/\.(vue|ts|tsx|js|jsx)$/i.test(document.fileName)) {
        return;
      }
      void refreshReactivity(false);
    }),
  );
}

async function refreshReactivity(revealSidebar) {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    void vscode.window.showWarningMessage('Vue Vet: open a workspace folder first.');
    return;
  }

  status.text = '$(sync~spin) Vue Vet';
  try {
    const configuredPath = vscode.workspace.getConfiguration('vue-vet').get('path', '');
    const report = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'Vue Vet: tracing reactivity…',
        cancellable: false,
      },
      () =>
        runReactivityScan({
          workspaceRoot: folder.uri.fsPath,
          configuredPath: typeof configuredPath === 'string' ? configuredPath : '',
        }),
    );

    modules = modulesFromReport(report);
    componentNav = componentNavFromReport(report);
    selection = null;
    treeProvider.setModules(modules, componentNav);
    applyDecorations();

    const count = modules.length;
    const facts = modules.reduce(
      (sum, module) =>
        sum +
        (module.binding_details?.length || 0) +
        (module.edge_details?.length || 0) +
        (module.template_details?.length || 0),
      0,
    );
    const componentEdges = componentNav.reduce(
      (sum, module) => sum + (module.uses?.length || 0),
      0,
    );
    status.text = `$(graph) Vue Vet ${count}m / ${facts}f`;
    status.tooltip = `Traced ${count} module(s), ${facts} structured facts, ${componentEdges} component use(s)`;

    if (revealSidebar) {
      await vscode.commands.executeCommand('vueVet.reactivity.focus');
    }
  } catch (error) {
    status.text = '$(error) Vue Vet';
    const message = error instanceof Error ? error.message : String(error);
    status.tooltip = `Vue Vet failed: ${message}`;
    const choice = await vscode.window.showErrorMessage(
      `Vue Vet: ${message}`,
      'Open Settings',
      'Retry',
    );
    if (choice === 'Open Settings') {
      await vscode.commands.executeCommand(
        'workbench.action.openSettings',
        'vue-vet.path',
      );
    } else if (choice === 'Retry') {
      await refreshReactivity(revealSidebar);
    }
  }
}

function clearHighlights() {
  selection = null;
  modules = [];
  componentNav = [];
  treeProvider.clear();
  applyDecorations();
  status.text = '$(graph) Vue Vet';
}

/**
 * @param {any} element
 * @param {'uses' | 'used_by'} direction
 */
async function inspectComponents(element, direction) {
  if (modules.length === 0 && componentNav.length === 0) {
    await refreshReactivity(false);
  }
  let moduleId = element?.moduleId;
  if (!moduleId) {
    const editor = vscode.window.activeTextEditor;
    const folder = vscode.workspace.workspaceFolders?.[0];
    if (!editor || !folder) {
      void vscode.window.showWarningMessage('Vue Vet: open a Vue file to inspect component usage.');
      return;
    }
    moduleId = normalizePath(path.relative(folder.uri.fsPath, editor.document.uri.fsPath));
  }
  const links = componentLinksFor(componentNav, moduleId, direction);
  if (links.length === 0) {
    void vscode.window.showInformationMessage(
      direction === 'uses'
        ? `Vue Vet: “${moduleId}” does not use other components (structural graph).`
        : `Vue Vet: no files template “${moduleId}” (structural graph · not prop dataflow).`,
    );
    return;
  }
  const picked = await vscode.window.showQuickPick(
    links.map((link) => ({
      label: direction === 'uses' ? `<${link.specifier}> → ${link.peer}` : `${link.peer} → <${link.specifier}>`,
      description: link.kind,
      detail: 'structural component edge · not prop dataflow',
      link,
    })),
    {
      title:
        direction === 'uses'
          ? `Components used by ${moduleId}`
          : `Who uses ${moduleId}`,
      matchOnDescription: true,
    },
  );
  if (!picked) {
    return;
  }
  // Evidence spans sit on the parent template tag.
  const evidenceModule = direction === 'uses' ? moduleId : picked.link.peer;
  await revealTreeNode({
    kind: 'component',
    key: `component:${direction}:${evidenceModule}@${picked.link.span?.offset ?? 0}`,
    moduleId: evidenceModule,
    span: picked.link.span,
  });
}

/**
 * @param {any} element
 */
async function revealTreeNode(element) {
  if (!element?.moduleId) {
    return;
  }
  selection = {
    kind: element.kind,
    key: element.key,
    moduleId: element.moduleId,
  };

  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    applyDecorations();
    return;
  }

  const targetPath = path.join(folder.uri.fsPath, element.moduleId);
  try {
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(targetPath));
    const editor = await vscode.window.showTextDocument(document, { preview: true });
    if (element.span && typeof element.span.offset === 'number') {
      const range = rangeFromByteSpan(document, element.span);
      editor.selection = new vscode.Selection(range.start, range.end);
      editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
    }
  } catch {
    // Module id may not map 1:1 onto disk (dual-script `#script`); still update highlights.
  }
  applyDecorations();
}

function applyDecorations() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return;
  }
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    return;
  }

  const relative = normalizePath(path.relative(folder.uri.fsPath, editor.document.uri.fsPath));
  const module = moduleForFile(modules, relative);
  const plan = decorationPlan(
    module,
    selection && module && selection.moduleId === module.id
      ? { kind: selection.kind, key: selection.key }
      : null,
  );

  /** @type {vscode.DecorationOptions[]} */
  const bindings = [];
  /** @type {vscode.DecorationOptions[]} */
  const edges = [];
  /** @type {vscode.DecorationOptions[]} */
  const selected = [];

  for (const item of plan) {
    const option = {
      range: rangeFromByteSpan(editor.document, item.span),
      hoverMessage: item.label,
    };
    if (item.role === 'selection') {
      selected.push(option);
    } else if (item.role === 'binding') {
      bindings.push(option);
    } else {
      edges.push(option);
    }
  }

  editor.setDecorations(bindingDecoration, bindings);
  editor.setDecorations(edgeDecoration, edges);
  editor.setDecorations(selectionDecoration, selected);
}

/**
 * @param {vscode.TextDocument} document
 * @param {vscode.Position} position
 */
function provideHover(document, position) {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder || modules.length === 0) {
    return null;
  }
  const relative = normalizePath(path.relative(folder.uri.fsPath, document.uri.fsPath));
  const module = moduleForFile(modules, relative);
  const utf16 = document.offsetAt(position);
  const offset = utf16OffsetToUtf8(document.getText(), utf16);
  const hit = hoverAtOffset(module, offset);
  if (!hit) {
    return null;
  }
  const markdown = new vscode.MarkdownString();
  markdown.appendMarkdown(`**${hit.label}**\n\n`);
  markdown.appendMarkdown(`_${hit.kind}_\n\n`);
  const covering = scopeAtOffset(module, offset);
  if (covering?.summary) {
    markdown.appendMarkdown(`${covering.summary}\n\n`);
  }
  markdown.appendMarkdown('Static reactivity fact from `vue-vet --print-reactivity`.');
  return new vscode.Hover(markdown);
}

async function explainScopeAtCursor() {
  const editor = vscode.window.activeTextEditor;
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!editor || !folder) {
    void vscode.window.showWarningMessage('Vue Vet: open a Vue/JS/TS file to explain a tracking scope.');
    return;
  }
  if (editor.document.uri.scheme !== 'file') {
    void vscode.window.showWarningMessage('Vue Vet: Explain Scope needs a file on disk.');
    return;
  }
  const relative = workspaceRelativePath(folder.uri.fsPath, editor.document.uri.fsPath);
  const utf16 = editor.document.offsetAt(editor.selection.active);
  const byteOffset = utf16OffsetToUtf8(editor.document.getText(), utf16);
  const query = explainScopeQuery(relative, byteOffset);
  if (!query) {
    void vscode.window.showWarningMessage('Vue Vet: Explain Scope needs a file inside the workspace.');
    return;
  }
  const configuredPath = vscode.workspace.getConfiguration('vue-vet').get('path', '');
  try {
    const payload = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'Vue Vet: explaining scope…',
        cancellable: false,
      },
      () =>
        runExplainScope({
          workspaceRoot: folder.uri.fsPath,
          query,
          configuredPath: typeof configuredPath === 'string' ? configuredPath : '',
        }),
    );
    const markdown = markdownFromScopeExplain(payload);
    const document = await vscode.workspace.openTextDocument({
      language: 'markdown',
      content: markdown,
    });
    await vscode.window.showTextDocument(document, { preview: true });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(`Vue Vet: ${message}`);
  }
}

/**
 * Map a Vue Vet UTF-8 byte span onto a VS Code Range (UTF-16 positions).
 * @param {vscode.TextDocument} document
 * @param {{ offset: number, length: number }} span
 */
function rangeFromByteSpan(document, span) {
  const text = document.getText();
  const startUtf16 = utf8OffsetToUtf16(text, span.offset);
  const endUtf16 = utf8OffsetToUtf16(text, span.offset + (span.length || 1));
  return new vscode.Range(document.positionAt(startUtf16), document.positionAt(endUtf16));
}

/**
 * @param {any} element tree node or undefined (editor context)
 * @param {'readers' | 'dependencies'} direction
 */
async function inspectBinding(element, direction) {
  if (modules.length === 0) {
    await refreshReactivity(false);
  }
  const resolved = resolveInspectTarget(element);
  if (!resolved) {
    void vscode.window.showWarningMessage(
      'Vue Vet: place the cursor on a traced binding, or right-click a binding in the Reactivity view.',
    );
    return;
  }
  const { module, bindingName } = resolved;
  const bareBinding = bindingName.includes('.') ? bindingName.split('.')[0] : bindingName;
  const items =
    direction === 'readers' ? inboundFor(module, bindingName) : outboundFor(module, bindingName);
  if (items.length === 0) {
    const empty =
      direction === 'readers'
        ? `No readers found for “${bindingName}”.`
        : `No outbound dependencies for “${bindingName}” (typical for a plain ref).`;
    void vscode.window.showInformationMessage(`Vue Vet: ${empty}`);
    selection = {
      kind: 'binding',
      key: `binding:${bindingName}@0`,
      moduleId: module.id,
    };
    // Prefer highlighting the binding declaration if present.
    const binding = (module.binding_details || []).find((item) => item.name === bareBinding);
    if (binding) {
      selection.key = bindingName.includes('.')
        ? `binding:${bindingName}@${binding.span.offset}`
        : `binding:${binding.name}@${binding.span.offset}`;
    }
    applyDecorations();
    return;
  }

  const picked = await vscode.window.showQuickPick(
    items.map((item) => ({
      label: item.label,
      description: item.kind,
      detail: direction === 'readers' ? `reads ${bindingName}` : `from ${bindingName}`,
      item,
    })),
    {
      title:
        direction === 'readers'
          ? `Who reads ${bindingName}`
          : `Dependencies of ${bindingName}`,
      matchOnDescription: true,
    },
  );
  if (!picked) {
    return;
  }

  selection = {
    kind: picked.item.key.startsWith('template:') ? 'template' : 'edge',
    key: picked.item.key,
    moduleId: module.id,
  };
  await revealTreeNode({
    kind: selection.kind,
    key: selection.key,
    moduleId: module.id,
    span: picked.item.span,
  });
}

/**
 * @param {any} element
 * @returns {{ module: import('./lib/model').ModuleDetail, bindingName: string } | null}
 */
function resolveInspectTarget(element) {
  if (element?.bindingName && element?.moduleId) {
    const module = modules.find((item) => item.id === element.moduleId);
    if (module) {
      return { module, bindingName: element.bindingName };
    }
  }

  const editor = vscode.window.activeTextEditor;
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!editor || !folder) {
    return null;
  }
  const relative = workspaceRelativePath(folder.uri.fsPath, editor.document.uri.fsPath);
  if (!relative) {
    return null;
  }
  const module = moduleForFile(modules, relative);
  if (!module) {
    return null;
  }
  const utf16 = editor.document.offsetAt(editor.selection.active);
  const byteOffset = utf16OffsetToUtf8(editor.document.getText(), utf16);
  const bindingName = bindingAtOffset(module, byteOffset);
  if (!bindingName) {
    return null;
  }
  return { module, bindingName };
}

/**
 * Workspace-relative path with `/` separators, or null when the file is outside.
 * @param {string} workspaceRoot
 * @param {string} filePath
 */
function workspaceRelativePath(workspaceRoot, filePath) {
  if (!workspaceRoot || !filePath) {
    return null;
  }
  const relative = path.relative(workspaceRoot, filePath);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) {
    return null;
  }
  return normalizePath(relative);
}

function deactivate() {
  modules = [];
  componentNav = [];
  selection = null;
}

module.exports = { activate, deactivate };
