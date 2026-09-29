import { hostTarget } from './host-target.mjs';
import { runNpm } from './npm.mjs';
import { tmpdir } from 'node:os';
import { mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const output = join(root, 'release', 'local');
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

const pack = (directory) => runNpm(['pack', '--ignore-scripts', '--pack-destination', output], {
  cwd: directory,
  stdio: 'inherit',
  env: { ...process.env, npm_config_cache: join(tmpdir(), 'systemeame-npm-cache') },
});

pack(join(root, 'npm', `systemeame-${hostTarget()}`));
pack(root);
console.log(`Local tarballs are ready in ${output}`);
