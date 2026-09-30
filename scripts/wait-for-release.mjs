import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';

// npm can acknowledge publication before its read endpoints serve the version.
// Wait for every optional package too, or installation can silently omit it.
const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
const pending = new Set([...Object.keys(pkg.optionalDependencies), pkg.name]);
// Registry propagation can exceed five minutes, especially for new packages.
for (let attempt = 0; attempt < 60 && pending.size; attempt++) {
  for (const name of pending) {
    const result = spawnSync('npm', ['view', `${name}@${pkg.version}`, 'version', '--json', '--prefer-online', '--fetch-retries=0'], {
      encoding: 'utf8', timeout: 15_000,
    });
    if (result.status === 0 && result.stdout.trim() === JSON.stringify(pkg.version)) pending.delete(name);
  }
  if (pending.size && attempt < 59) {
    console.log(`Waiting for npm to serve ${pkg.version}: ${[...pending].join(', ')}`);
    await setTimeout(15_000);
  }
}
if (pending.size) throw new Error(`Published packages are not visible yet: ${[...pending].join(', ')}. Retry the release workflow.`);
console.log(`All packages for ${pkg.version} are visible on npm.`);
