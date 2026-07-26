'use strict';

const vscode = require('vscode');
const { buildTree } = require('./model');

class ReactivityTreeProvider {
  constructor() {
    this._onDidChangeTreeData = new vscode.EventEmitter();
    this.onDidChangeTreeData = this._onDidChangeTreeData.event;
    /** @type {import('./model').ModuleDetail[] | ReturnType<typeof buildTree>} */
    this._roots = [];
    this._modules = [];
  }

  /**
   * @param {import('./model').ModuleDetail[]} modules
   */
  setModules(modules) {
    this._modules = modules;
    this._roots = buildTree(modules);
    this._onDidChangeTreeData.fire();
  }

  clear() {
    this._modules = [];
    this._roots = [];
    this._onDidChangeTreeData.fire();
  }

  getModules() {
    return this._modules;
  }

  getTreeItem(element) {
    const item = new vscode.TreeItem(
      element.label,
      element.children?.length
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None,
    );
    item.description = element.description;
    item.tooltip = element.label;
    item.contextValue = element.kind;
    if (element.kind === 'module') {
      item.iconPath = new vscode.ThemeIcon('file-code');
    } else if (element.kind === 'binding') {
      item.iconPath = new vscode.ThemeIcon('symbol-variable');
    } else if (element.kind === 'edge') {
      item.iconPath = new vscode.ThemeIcon('arrow-right');
    } else if (element.kind === 'template') {
      item.iconPath = new vscode.ThemeIcon('code');
    } else {
      item.iconPath = new vscode.ThemeIcon('folder');
    }
    if (element.kind === 'binding' || element.kind === 'edge' || element.kind === 'template') {
      item.command = {
        command: 'vue-vet.revealTreeNode',
        title: 'Reveal',
        arguments: [element],
      };
    }
    return item;
  }

  getChildren(element) {
    if (!element) {
      return this._roots;
    }
    return element.children || [];
  }
}

module.exports = { ReactivityTreeProvider };
