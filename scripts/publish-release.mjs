import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

// Verify every archive before publishing anything. Native packages come first.
execFileSync(process.execPath, ['scripts/verify-release.mjs'], { stdio: 'inherit' });
const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
assert.equal(process.env.RELEASE_TAG, `v${pkg.version}`);
for (const name of [...Object.keys(pkg.optionalDependencies).sort(), pkg.name]) {
  const archive = `./release/${name}-${pkg.version}.tgz`;
  const result = spawnSync('npm', ['view', `${name}@${pkg.version}`, 'dist.integrity', '--json'], { encoding: 'utf8' });
  if (result.status === 0) {
    const integrity = `sha512-${createHash('sha512').update(readFileSync(archive)).digest('base64')}`;
    assert.equal(JSON.parse(result.stdout), integrity, `${name} already exists with different bytes; refusing to overwrite or skip it.`);
    console.log(`${name}@${pkg.version} already published with matching integrity.`);
  } else {
    const error = JSON.parse(result.stdout || '{}');
    if (error.error?.code !== 'E404') throw new Error(result.stderr || result.stdout);
    execFileSync('npm', ['publish', archive, '--access', 'public', '--provenance'], { stdio: 'inherit' });
  }
}
