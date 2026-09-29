import { runNpm } from './npm.mjs';
import { tmpdir } from 'node:os';
import { mkdir, readFile, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const output = join(root, 'release', process.env.GITHUB_SHA ?? 'local');
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

const pack = (directory) => runNpm(['pack', '--ignore-scripts', '--pack-destination', output], {
  cwd: directory,
  stdio: 'inherit',
  env: { ...process.env, npm_config_cache: join(tmpdir(), 'systemeame-npm-cache') },
});

const manifest = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
for (const name of Object.keys(manifest.optionalDependencies)) pack(join(root, 'npm', name));
pack(root);
console.log(`Release tarballs are ready in ${output}`);
