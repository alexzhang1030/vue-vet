'use strict';

const path = require('node:path');
const vscode = require('vscode');
const { runReactivityScan } = require('./lib/cli');
const {
  modulesFromReport,
  moduleForFile,
  decorationPlan,
  hoverAtOffset,
  normalizePath,
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
/** @type {{ kind: 'binding' | 'edge' | 'template', key: string, moduleId: string } | null} */
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
    selection = null;
    treeProvider.setModules(modules);
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
    status.text = `$(graph) Vue Vet ${count}m / ${facts}f`;
    status.tooltip = `Traced ${count} module(s), ${facts} structured facts`;

    if (revealSidebar) {
      await vscode.commands.executeCommand('vueVet.reactivity.focus');
    }
  } catch (error) {
    status.text = '$(error) Vue Vet';
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(`Vue Vet: ${message}`);
  }
}

function clearHighlights() {
  selection = null;
  modules = [];
  treeProvider.clear();
  applyDecorations();
  status.text = '$(graph) Vue Vet';
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
      const start = document.positionAt(element.span.offset);
      const end = document.positionAt(element.span.offset + (element.span.length || 1));
      editor.selection = new vscode.Selection(start, end);
      editor.revealRange(new vscode.Range(start, end), vscode.TextEditorRevealType.InCenter);
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
    const start = editor.document.positionAt(item.span.offset);
    const end = editor.document.positionAt(item.span.offset + item.span.length);
    const option = {
      range: new vscode.Range(start, end),
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
  const offset = document.offsetAt(position);
  const hit = hoverAtOffset(module, offset);
  if (!hit) {
    return null;
  }
  const markdown = new vscode.MarkdownString();
  markdown.appendMarkdown(`**${hit.label}**\n\n`);
  markdown.appendMarkdown(`_${hit.kind}_\n\n`);
  markdown.appendMarkdown('Static reactivity fact from `vue-vet --print-reactivity`.');
  return new vscode.Hover(markdown);
}

function deactivate() {
  modules = [];
  selection = null;
}

module.exports = { activate, deactivate };
