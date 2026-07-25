#!/usr/bin/env node
'use strict';

const { spawn } = require('node:child_process');
const { resolveBinary } = require('../lib/resolve.js');

function main() {
  let binary;
  try {
    binary = resolveBinary({ requireFrom: __filename });
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(2);
  }

  const child = spawn(binary, process.argv.slice(2), {
    stdio: 'inherit',
    windowsHide: false,
  });

  const forward = (signal) => {
    if (!child.killed) {
      child.kill(signal);
    }
  };
  process.on('SIGINT', () => forward('SIGINT'));
  process.on('SIGTERM', () => forward('SIGTERM'));

  child.on('error', (error) => {
    console.error(`Failed to start vue-vet native binary at ${binary}: ${error.message}`);
    process.exit(2);
  });

  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 1);
  });
}

main();
