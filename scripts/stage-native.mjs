import { chmod, copyFile, mkdir, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { hostTarget } from './host-target.mjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const argument = (name) => {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
};
const target = argument('--target') ?? hostTarget();
const sourceArgument = argument('--source');

const targets = {
  'linux-x64-gnu': { platform: 'linux', architecture: 'x64', binary: 'systemeame-native' },
  'linux-arm64-gnu': { platform: 'linux', architecture: 'arm64', binary: 'systemeame-native' },
  'linux-x64-musl': { platform: 'linux', architecture: 'x64', binary: 'systemeame-native' },
  'linux-arm64-musl': { platform: 'linux', architecture: 'arm64', binary: 'systemeame-native' },
  'darwin-x64': { platform: 'darwin', architecture: 'x64', binary: 'systemeame-native' },
  'darwin-arm64': { platform: 'darwin', architecture: 'arm64', binary: 'systemeame-native' },
  'win32-x64': { platform: 'win32', architecture: 'x64', binary: 'systemeame-native.exe' },
  'win32-arm64': { platform: 'win32', architecture: 'arm64', binary: 'systemeame-native.exe' },
};

if (!target || !targets[target]) {
  throw new Error(`Pass a supported --target: ${Object.keys(targets).join(', ')}`);
}
const details = targets[target];

const source = sourceArgument ? join(root, sourceArgument) : join(root, 'target', 'release', details.binary);
const packageDirectory = join(root, 'npm', `systemeame-${target}`);
const destination = join(packageDirectory, 'bin', details.binary);
try {
  await stat(source);
} catch {
  throw new Error(`Native build output is missing: ${source}`);
}
await mkdir(dirname(destination), { recursive: true });
await copyFile(source, destination);
await copyFile(join(root, 'LICENSE'), join(packageDirectory, 'LICENSE'));
if (details.platform !== 'win32') await chmod(destination, 0o755);
console.log(`Staged ${destination}`);
