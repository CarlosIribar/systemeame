import test from 'node:test';
import assert from 'node:assert/strict';
import { nextVersion } from '../scripts/auto-release.mjs';

test('first release uses the configured minimum without reusing reserved versions', () => {
  assert.equal(nextVersion('0.1.1', ['0.1.0']), '0.1.1');
  assert.equal(nextVersion('0.1.1', ['0.1.0', '0.1.1']), '0.1.2');
});
test('patch increments numerically and respects an explicit major or minor bump', () => {
  assert.equal(nextVersion('0.1.1', ['0.1.9', '0.1.10']), '0.1.11');
  assert.equal(nextVersion('0.2.0', ['0.1.10']), '0.2.0');
  assert.equal(nextVersion('1.0.0', []), '1.0.0');
  assert.throws(() => nextVersion('invalid', []));
});

import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

test('release reservation is retry-safe, advances patches, and never pushes main', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'systemeame-release-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const repo = join(dir, 'repo');
  const remote = join(dir, 'remote.git');
  mkdirSync(repo);
  const git = (...args) => execFileSync('git', args, { cwd: repo, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git('init', '--bare', remote);
  git('init', '-b', 'main');
  git('config', 'user.name', 'Test');
  git('config', 'user.email', 'test@example.invalid');
  for (const name of ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', 'scripts', 'npm']) {
    cpSync(name, join(repo, name), { recursive: true, filter: (source) => !source.includes('/bin/') && !source.endsWith('/bin') });
  }
  git('add', '.');
  git('commit', '-m', 'source');
  git('remote', 'add', 'origin', remote);
  git('push', 'origin', 'main');
  const first = git('rev-parse', 'HEAD');
  const output = join(dir, 'output');
  const reserve = (source) => {
    git('checkout', '--detach', source);
    writeFileSync(output, '');
    execFileSync(process.execPath, ['scripts/auto-release.mjs', '--reserve'], { cwd: repo, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, GITHUB_REF: 'refs/heads/main', GITHUB_SHA: source, GITHUB_OUTPUT: output } });
    return Object.fromEntries(readFileSync(output, 'utf8').trim().split('\n').map((line) => line.split('=')));
  };
  const initial = reserve(first);
  const tagged = git('rev-parse', `${initial.tag}^{commit}`);
  assert.deepEqual(reserve(first), initial);
  assert.equal(git('rev-parse', `${initial.tag}^{commit}`), tagged);
  assert.equal(git('ls-remote', 'origin', 'refs/heads/main').split(/\s/)[0], first);
  git('checkout', 'main');
  writeFileSync(join(repo, 'change.txt'), 'next source change');
  git('add', 'change.txt');
  git('commit', '-m', 'next change');
  const second = git('rev-parse', 'HEAD');
  const next = reserve(second);
  assert.equal(next.version, nextVersion(initial.version, [initial.version]));
  const snapshot = JSON.parse(git('show', `${next.tag}:package.json`));
  assert.equal(snapshot.version, next.version);
  assert.ok(Object.values(snapshot.optionalDependencies).every((version) => version === next.version));
  assert.equal(git('ls-remote', 'origin', 'refs/heads/main').split(/\s/)[0], first);
});
