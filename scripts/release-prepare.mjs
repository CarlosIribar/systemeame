import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const versionIndex = process.argv.indexOf('--version');
const version = versionIndex === -1 ? undefined : process.argv[versionIndex + 1];
if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error('Usage: npm run release:prepare -- --version <semver>');
}

const updateJson = async (path, transform) => {
  const value = JSON.parse(await readFile(path, 'utf8'));
  transform(value);
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`);
};

await updateJson(join(root, 'package.json'), (pkg) => {
  pkg.version = version;
  for (const name of Object.keys(pkg.optionalDependencies)) pkg.optionalDependencies[name] = version;
});
for (const directory of ['systemeame-linux-x64-gnu', 'systemeame-linux-arm64-gnu', 'systemeame-linux-x64-musl', 'systemeame-linux-arm64-musl', 'systemeame-darwin-x64', 'systemeame-darwin-arm64', 'systemeame-win32-x64', 'systemeame-win32-arm64']) {
  await updateJson(join(root, 'npm', directory, 'package.json'), (pkg) => { pkg.version = version; });
}
const cargoPath = join(root, 'Cargo.toml');
const cargo = await readFile(cargoPath, 'utf8');
await writeFile(cargoPath, cargo.replace(/^version = ".*"$/m, `version = "${version}"`));
console.log(`Prepared version ${version}. Run npm install --package-lock-only, tests, commit, and tag v${version}.`);
