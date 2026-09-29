import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

for (const arch of ['x64', 'arm64']) {
  test(`Windows ${arch} resolves installed and locally staged .exe binaries`, (t) => {
    const root = mkdtempSync(join(tmpdir(), 'systemeame-resolve-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    mkdirSync(join(root, 'lib/native'), { recursive: true });
    writeFileSync(join(root, 'package.json'), JSON.stringify({ type: 'module' }));
    const module = join(root, 'lib/native/resolve.js');
    copyFileSync('lib/native/resolve.js', module);
    const name = `systemeame-win32-${arch}`;
    const installed = join(root, 'node_modules', name);
    mkdirSync(join(installed, 'bin'), { recursive: true });
    writeFileSync(join(installed, 'package.json'), JSON.stringify({ name, version: '1.0.0' }));
    writeFileSync(join(installed, 'bin/systemeame-native.exe'), 'fixture');
    const resolve = () => execFileSync(process.execPath, ['--input-type=module', '-e', `
      Object.defineProperty(process, 'platform', { value: 'win32' });
      Object.defineProperty(process, 'arch', { value: ${JSON.stringify(arch)} });
      const { resolveNativeBinary } = await import(${JSON.stringify(pathToFileURL(module).href)});
      console.log(resolveNativeBinary());
    `], { encoding: 'utf8' }).trim();
    assert.equal(resolve(), join(installed, 'bin/systemeame-native.exe'));
    const staged = join(root, 'npm', name, 'bin');
    mkdirSync(staged, { recursive: true });
    writeFileSync(join(staged, 'systemeame-native.exe'), 'fixture');
    assert.equal(resolve(), join(staged, 'systemeame-native.exe'));
  });
}
