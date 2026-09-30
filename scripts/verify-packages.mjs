import { access, readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const targetIndex = process.argv.indexOf('--target');
const target = targetIndex === -1 ? 'linux-x64-gnu' : process.argv[targetIndex + 1];
const platforms = {
  'linux-x64-gnu': ['linux', 'x64', 'systemeame-native'],
  'linux-arm64-gnu': ['linux', 'arm64', 'systemeame-native'],
  'linux-x64-musl': ['linux', 'x64', 'systemeame-native'],
  'linux-arm64-musl': ['linux', 'arm64', 'systemeame-native'],
  'darwin-x64': ['darwin', 'x64', 'systemeame-native'],
  'darwin-arm64': ['darwin', 'arm64', 'systemeame-native'],
  'win32-x64': ['win32', 'x64', 'systemeame-native.exe'],
  'win32-arm64': ['win32', 'arm64', 'systemeame-native.exe'],
};
if (!platforms[target]) throw new Error(`Unknown target ${target}`);
const [os, cpu, binaryName] = platforms[target];
const rootPackage = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
const packageNameForTarget = {
  'win32-x64': '@carlosiribar/systemeame-win32-x64',
  'win32-arm64': '@carlosiribar/systemeame-win32-arm64',
};
const packageDirectory = join(root, 'npm', packageNameForTarget[target] ?? `systemeame-${target}`);
const nativePackage = JSON.parse(await readFile(join(packageDirectory, 'package.json'), 'utf8'));
if (nativePackage.version !== rootPackage.version) throw new Error(`${nativePackage.name} version must equal the plugin version.`);
if (nativePackage.os?.[0] !== os || nativePackage.cpu?.[0] !== cpu) throw new Error(`${nativePackage.name} has incorrect os/cpu metadata.`);
if (rootPackage.optionalDependencies[nativePackage.name] !== rootPackage.version) throw new Error(`${nativePackage.name} must be an exact optional dependency.`);
const binary = join(packageDirectory, 'bin', binaryName);
await access(binary);
const info = await stat(binary);
if (!info.isFile()) throw new Error('Staged native binary is not a file.');
if (os !== 'win32' && (info.mode & 0o111) === 0) throw new Error('Staged native binary is not executable.');
console.log(`${nativePackage.name} is staged with matching version and platform metadata.`);
