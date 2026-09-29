import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);

function targetName(): string {
  if (process.platform !== 'linux') return `${process.platform}-${process.arch}`;
  // Node exposes the glibc runtime only on glibc-based Linux. Undefined is the
  // documented shape on musl, which lets one npm install support Alpine too.
  const report = process.report?.getReport() as { header?: { glibcVersionRuntime?: string } } | undefined;
  const libc = report?.header?.glibcVersionRuntime ? 'gnu' : 'musl';
  return `linux-${process.arch}-${libc}`;
}

const packageForTarget: Record<string, string> = {
  'linux-x64-gnu': 'systemeame-linux-x64-gnu',
  'linux-arm64-gnu': 'systemeame-linux-arm64-gnu',
  'linux-x64-musl': 'systemeame-linux-x64-musl',
  'linux-arm64-musl': 'systemeame-linux-arm64-musl',
  'darwin-x64': 'systemeame-darwin-x64',
  'darwin-arm64': 'systemeame-darwin-arm64',
  'win32-x64': 'systemeame-win32-x64',
  'win32-arm64': 'systemeame-win32-arm64',
};

export function resolveNativeBinary(): string {
  const target = targetName();
  const packageName = packageForTarget[target];
  if (!packageName) throw new Error(`Unsupported platform ${target}. See the systemeame support matrix.`);
  try {
    const manifest = require.resolve(`${packageName}/package.json`);
    const binary = join(dirname(manifest), 'bin', process.platform === 'win32' ? 'systemeame-native.exe' : 'systemeame-native');
    if (existsSync(binary)) return binary;
  } catch { /* produce one actionable error below */ }
  // `sf plugins link` loads this package straight from the repository. The
  // staged artifact is deliberately outside the npm allowlist, so a published
  // tarball can never take this branch. It makes local Salesforce-project
  // smoke tests possible without pretending that a registry release exists.
  const linkedBinary = join(
    dirname(dirname(dirname(fileURLToPath(import.meta.url)))),
    'npm', packageName, 'bin', process.platform === 'win32' ? 'systemeame-native.exe' : 'systemeame-native',
  );
  if (existsSync(linkedBinary)) return linkedBinary;
  throw new Error(`The native package ${packageName} is missing or incomplete. Reinstall systemeame without omitting optional dependencies.`);
}
