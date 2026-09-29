import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const versionIndex = process.argv.indexOf('--version');
const version = versionIndex === -1 ? undefined : process.argv[versionIndex + 1];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error('Usage: npm run release:prepare -- --version <major.minor.patch>');
}
const readJson = async (path) => JSON.parse(await readFile(join(root, path), 'utf8'));
const writeJson = async (path, value) => writeFile(join(root, path), `${JSON.stringify(value, null, 2)}\n`);
const pkg = await readJson('package.json');
pkg.version = version;
for (const name of Object.keys(pkg.optionalDependencies)) pkg.optionalDependencies[name] = version;
await writeJson('package.json', pkg);
const lock = await readJson('package-lock.json');
lock.version = version;
Object.assign(lock.packages[''], { version, optionalDependencies: pkg.optionalDependencies, os: pkg.os, cpu: pkg.cpu });
for (const key of Object.keys(lock.packages)) {
  if (key.startsWith('node_modules/systemeame-') && !pkg.optionalDependencies[key.slice('node_modules/'.length)]) delete lock.packages[key];
}
for (const name of Object.keys(pkg.optionalDependencies)) {
  const native = await readJson(`npm/${name}/package.json`);
  native.version = version;
  await writeJson(`npm/${name}/package.json`, native);
  const previous = lock.packages[`node_modules/${name}`];
  // Own packages may not exist yet. Pin their version and platform from source;
  // never retain an integrity hash from a different release.
  lock.packages[`node_modules/${name}`] = {
    version, resolved: `https://registry.npmjs.org/${name}/-/${name}-${version}.tgz`,
    ...(previous?.version === version && previous.integrity ? { integrity: previous.integrity } : {}),
    cpu: native.cpu, license: native.license, optional: true, os: native.os,
    ...(native.libc ? { libc: native.libc } : {}), bin: native.bin,
  };
}
await writeJson('package-lock.json', lock);
const cargoPath = join(root, 'Cargo.toml');
await writeFile(cargoPath, (await readFile(cargoPath, 'utf8')).replace(/^version = ".*"$/m, `version = "${version}"`));
const cargoLockPath = join(root, 'Cargo.lock');
await writeFile(cargoLockPath, (await readFile(cargoLockPath, 'utf8')).replace(/(name = "systemeame-(?:core|native)"\nversion = ")[^"]+("\n)/g, `$1${version}$2`));
console.log(`Prepared ${version}, including npm and Cargo lockfiles.`);
