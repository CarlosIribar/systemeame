import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { runNative } from '../../lib/native/run.js';

const source = 'public class Example { void run(Account a) { insert a; Object rows = [SELECT Id FROM Account WHERE Id = :a.Id ORDER BY Name LIMIT 1]; Database.query(\'SELECT Id FROM Account\'); } }';
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'systemeame-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, 'force-app'));
  writeFileSync(join(dir, 'sfdx-project.json'), JSON.stringify({ packageDirectories: [{ path: 'force-app' }] }));
  const file = join(dir, 'force-app', 'Example.cls');
  writeFileSync(file, source);
  return { dir, file };
}

test('dry-run and check preserve files; applying is idempotent', async (t) => {
  const { dir, file } = fixture(t);
  const request = { protocolVersion: 1, projectDir: dir, all: true };
  const dry = await runNative({ ...request, dryRun: true });
  assert.equal(dry.result.proposedEdits, 3);
  assert.equal(readFileSync(file, 'utf8'), source);
  assert.equal((await runNative({ ...request, check: true })).exitCode, 1);
  assert.equal(readFileSync(file, 'utf8'), source);
  assert.equal((await runNative(request)).result.changedFiles, 1);
  assert.match(readFileSync(file, 'utf8'), /insert as system a/);
  assert.match(readFileSync(file, 'utf8'), /WHERE Id = :a.Id WITH SYSTEM_MODE ORDER BY Name LIMIT 1/);
  assert.match(readFileSync(file, 'utf8'), /System.AccessLevel.SYSTEM_MODE/);
  assert.equal((await runNative({ ...request, check: true })).exitCode, 0);
});

test('staged scope rejects partial staging and never changes index', async (t) => {
  const { dir, file } = fixture(t);
  const git = (...args) => execFileSync('git', ['-C', dir, ...args], { encoding: 'utf8' });
  git('init', '--quiet');
  git('config', 'core.autocrlf', 'false');
  git('add', 'force-app/Example.cls');
  const index = git('show', ':force-app/Example.cls');
  writeFileSync(file, source + '\n');
  const request = { protocolVersion: 1, projectDir: dir };
  await assert.rejects(runNative(request), /PARTIAL_STAGE/);
  assert.equal(readFileSync(file, 'utf8'), source + '\n');
  assert.equal((await runNative({ ...request, allowUnstaged: true })).exitCode, 0);
  assert.equal(git('show', ':force-app/Example.cls'), index);
});

test('invalid UTF-8 is reported while valid files continue', async (t) => {
  const { dir } = fixture(t);
  const file = join(dir, 'force-app', 'Invalid.cls');
  writeFileSync(file, Buffer.from([0xff]));
  const result = await runNative({ protocolVersion: 1, projectDir: dir, all: true, dryRun: true });
  assert.equal(result.exitCode, 1);
  assert.equal(result.result.proposedEdits, 3);
  assert.ok(result.result.diagnostics.some(({ code }) => code === 'INVALID_UTF8'));
  assert.deepEqual(readFileSync(file), Buffer.from([0xff]));
});

test('inline locator modes are exclusive, existing duplicates are repaired, and check agrees', async (t) => {
  const { dir, file } = fixture(t);
  const input = `public class Example {
    Object run(Integer days) {
      return Database.getQueryLocator(
        [SELECT Id, Name FROM Account WHERE Age__c = :days WITH SYSTEM_MODE],
        System.AccessLevel.SYSTEM_MODE
      );
    }
    Object missing() { return Database.getQueryLocator([SELECT Id FROM Account]); }
    Object explicitMode(System.AccessLevel mode) {
      return Database.getQueryLocator([SELECT Id FROM Account], mode);
    }
  }`;
  writeFileSync(file, input);
  const request = { protocolVersion: 1, projectDir: dir, all: true };
  assert.equal((await runNative({ ...request, check: true })).exitCode, 1);
  assert.equal(readFileSync(file, 'utf8'), input);
  const applied = await runNative(request);
  assert.equal(applied.exitCode, 0);
  assert.equal(applied.result.changedFiles, 1);
  const fixed = readFileSync(file, 'utf8');
  assert.match(fixed, /WHERE Age__c = :days WITH SYSTEM_MODE\]/);
  assert.doesNotMatch(fixed, /System.AccessLevel.SYSTEM_MODE/);
  assert.match(fixed, /getQueryLocator\(\[SELECT Id FROM Account WITH SYSTEM_MODE\]\)/);
  assert.match(fixed, /getQueryLocator\(\[SELECT Id FROM Account\], mode\)/);
  assert.equal((await runNative({ ...request, check: true })).exitCode, 0);
  assert.equal((await runNative(request)).result.changedFiles, 0);
  assert.equal(readFileSync(file, 'utf8'), fixed);
});

test('unresolved dynamic text requires review without inserting a mode', async (t) => {
  const { dir, file } = fixture(t);
  const input = 'class Example { Object run(String queryText) { return Database.query(queryText); } }';
  writeFileSync(file, input);
  const result = await runNative({ protocolVersion: 1, projectDir: dir, all: true });
  assert.equal(result.exitCode, 1);
  assert.equal(result.result.changedFiles, 0);
  assert.ok(result.result.diagnostics.some(({ code }) => code === 'DYNAMIC_QUERY_UNRESOLVED'));
  assert.equal(readFileSync(file, 'utf8'), input);
});
