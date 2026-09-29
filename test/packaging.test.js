import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runNpm } from '../scripts/npm.mjs';
import { assertFinished } from '../lib/native/protocol.js';

test('published plugin contains runtime files only', () => {
  const [pack] = JSON.parse(runNpm(['pack', '--dry-run', '--json', '--ignore-scripts', '--cache', join(tmpdir(), 'systemeame-npm-cache')], { encoding: 'utf8' }));
  const files = pack.files.map(({ path }) => path);
  assert.ok(files.includes('lib/commands/apex/system-mode/fix.js'));
  assert.ok(files.includes('LICENSE'));
  for (const path of files) {
    assert.ok(path.startsWith('lib/') || ['package.json', 'README.md', 'LICENSE', 'CHANGELOG.md'].includes(path), path);
  }
});

test('all native dependencies have matching versions and metadata', () => {
  const root = JSON.parse(readFileSync('package.json', 'utf8'));
  assert.equal(Object.keys(root.optionalDependencies).length, 8);
  assert.deepEqual(root.os, ['linux', 'darwin', 'win32']);

  for (const [name, version] of Object.entries(root.optionalDependencies)) {
    const native = JSON.parse(readFileSync(`npm/${name}/package.json`, 'utf8'));
    assert.equal(native.name, name);
    assert.equal(native.version, root.version);
    assert.equal(version, root.version);
    assert.deepEqual(native.repository, root.repository);
    assert.deepEqual(native.files, ['bin', 'README.md', 'LICENSE']);
  }
});

test('protocol rejects incompatible native events', () => {
  assert.throws(() => assertFinished({ protocolVersion: 2, event: 'finished', exitCode: 0 }));
  assert.throws(() => assertFinished(null));
  assert.throws(() => assertFinished({ protocolVersion: 1, event: 'progress', exitCode: 0 }));
});
