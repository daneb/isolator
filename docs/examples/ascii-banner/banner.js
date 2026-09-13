#!/usr/bin/env node
'use strict';

const MAX_CONTENT_WIDTH = 96;

function sanitize(text) {
  return text.replace(/[\x00-\x09\x0B-\x1F\x7F]/g, '');
}

function truncateLine(line) {
  if (line.length <= MAX_CONTENT_WIDTH) return line;
  return `${line.slice(0, MAX_CONTENT_WIDTH - 3)}...`;
}

function buildBanner(text) {
  const sanitized = sanitize(text);
  if (sanitized.trim().length === 0) {
    throw new Error('no input text provided');
  }

  const lines = sanitized.split('\n').map((line) => truncateLine(line.toUpperCase()));
  const width = lines.reduce((max, line) => Math.max(max, line.length), 0);

  const border = `+${'-'.repeat(width + 2)}+`;
  const body = lines.map((line) => `| ${line.padEnd(width)} |`).join('\n');

  return `${border}\n${body}\n${border}`;
}

function readStdin() {
  const fs = require('fs');
  try {
    return fs.readFileSync(0, 'utf8');
  } catch (err) {
    return '';
  }
}

function main() {
  const args = process.argv.slice(2);
  const input = args.length > 0 ? args.join(' ') : readStdin();

  try {
    const banner = buildBanner(input);
    process.stdout.write(`${banner}\n`);
  } catch (err) {
    process.stderr.write(`Error: ${err.message}\n`);
    process.exitCode = 1;
  }
}

if (require.main === module) {
  main();
}

module.exports = { sanitize, truncateLine, buildBanner, MAX_CONTENT_WIDTH };
