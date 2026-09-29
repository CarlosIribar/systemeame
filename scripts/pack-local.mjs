import { execFileSync } from 'node:child_process';
import { mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const output = join(root, 'release', 'local');
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const pack = (directory) => execFileSync(npm, ['pack', '--ignore-scripts', '--pack-destination', output], {
  cwd: directory,
  stdio: 'inherit',
  env: { ...process.env, npm_config_cache: '/tmp/systemeame-npm-cache' },
});

pack(join(root, 'npm', 'systemeame-linux-x64-gnu'));
pack(root);
console.log(`Local tarballs are ready in ${output}`);
