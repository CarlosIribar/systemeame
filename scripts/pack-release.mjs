import { execFileSync } from 'node:child_process';
import { mkdir, readdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const output = join(root, 'release', process.env.GITHUB_SHA ?? 'local');
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const pack = (directory) => execFileSync(npm, ['pack', '--ignore-scripts', '--pack-destination', output], {
  cwd: directory,
  stdio: 'inherit',
  env: { ...process.env, npm_config_cache: '/tmp/systemeame-npm-cache' },
});

for (const entry of await readdir(join(root, 'npm'), { withFileTypes: true })) {
  if (entry.isDirectory()) pack(join(root, 'npm', entry.name));
}
pack(root);
console.log(`Release tarballs are ready in ${output}`);
