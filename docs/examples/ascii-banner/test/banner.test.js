'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { sanitize, buildBanner } = require('../banner.js');

const BANNER_PATH = path.join(__dirname, '..', 'banner.js');

test('renders a bordered box around uppercased text', () => {
  const output = buildBanner('hello');
  const lines = output.split('\n');
  assert.equal(lines[0], '+-------+');
  assert.equal(lines[1], '| HELLO |');
  assert.equal(lines[2], '+-------+');
});

test('throws a clear error for empty input', () => {
  assert.throws(() => buildBanner(''), /no input text provided/);
  assert.throws(() => buildBanner('   \n  '), /no input text provided/);
});

test('CLI exits non-zero with a stderr message and no stack trace on empty input', () => {
  const result = spawnSync(process.execPath, [BANNER_PATH], {
    input: '',
    encoding: 'utf8',
  });

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Error: no input text provided/);
  assert.doesNotMatch(result.stderr, /at Object/);
  assert.equal(result.stdout, '');
});

test('strips ASCII control characters before rendering', () => {
  const withControls = 'hi\x07\x1Bthere\x7F';
  assert.equal(sanitize(withControls), 'hithere');

  const output = buildBanner('a\x00b');
  assert.ok(output.includes('AB'));
});

test('supports multi-line input, sizing the box to the longest line', () => {
  const output = buildBanner('hi\nworld');
  const lines = output.split('\n');
  assert.equal(lines[0], '+-------+');
  assert.equal(lines[1], '| HI    |');
  assert.equal(lines[2], '| WORLD |');
  assert.equal(lines[3], '+-------+');
});

test('gracefully truncates lines longer than the max width', () => {
  const longLine = 'x'.repeat(150);
  const output = buildBanner(longLine);
  const contentLine = output.split('\n')[1];
  const content = contentLine.slice(2, -2);
  assert.equal(content.length, 96);
  assert.ok(content.endsWith('...'));
});
