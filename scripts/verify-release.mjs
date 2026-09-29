import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const root = JSON.parse(readFileSync('package.json', 'utf8'));
const expected = [root, ...Object.keys(root.optionalDependencies).map((name) =>
  JSON.parse(readFileSync(join('npm', name, 'package.json'), 'utf8')))];
const archives = readdirSync('release').filter((name) => name.endsWith('.tgz')).sort();
assert.deepEqual(archives, expected.map((pkg) => `${pkg.name}-${root.version}.tgz`).sort());
const cargo = readFileSync('Cargo.toml', 'utf8');
assert.match(cargo, new RegExp(`^version = "${root.version.replaceAll('.', '\\.')}"$`, 'm'));
if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
  assert.equal(process.env.GITHUB_REF_NAME, `v${root.version}`);
}
for (const pkg of expected) {
  assert.equal(pkg.version, root.version);
  const file = join('release', `${pkg.name}-${pkg.version}.tgz`);
  const manifest = JSON.parse(execFileSync('tar', ['-xOf', file, 'package/package.json'], { encoding: 'utf8' }));
  assert.equal(manifest.name, pkg.name);
  assert.equal(manifest.version, pkg.version);
  const entries = execFileSync('tar', ['-tzf', file], { encoding: 'utf8' }).trim().split('\n');
  for (const entry of entries) {
    const path = entry.replace(/^package\//, '');
    assert.ok(/^(package\.json|README\.md|LICENSE|CHANGELOG\.md)$/.test(path)
      || path.startsWith(pkg.name === root.name ? 'lib/' : 'bin/'), `Unexpected package file: ${entry}`);
  }
  assert.ok(entries.includes('package/LICENSE'));
  if (pkg.name === root.name) {
    assert.deepEqual(manifest.optionalDependencies, root.optionalDependencies);
    assert.ok(entries.includes('package/lib/commands/apex/system-mode/fix.js'));
  } else {
    assert.equal(root.optionalDependencies[pkg.name], pkg.version);
    assert.deepEqual(manifest.os, pkg.os);
    assert.deepEqual(manifest.cpu, pkg.cpu);
    assert.deepEqual(manifest.libc, pkg.libc);
    const binary = pkg.os[0] === 'win32' ? 'systemeame-native.exe' : 'systemeame-native';
    assert.ok(entries.includes(`package/bin/${binary}`));
  }
}
console.log(`Verified ${archives.length} release archives for ${root.version}.`);
