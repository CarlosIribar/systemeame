import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

export function nextVersion(minimum, versions) {
  const parse = (value) => {
    assert.match(value, /^\d+\.\d+\.\d+$/);
    return value.split('.').map(Number);
  };
  const compare = (a, b) => { for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i]; return 0; };
  const baseline = parse(minimum);
  const latest = versions.map(parse).sort(compare).at(-1);
  if (!latest || compare(baseline, latest) > 0) return minimum;
  latest[2] += 1;
  return latest.join('.');
}

if (process.argv.includes('--reserve')) {
  assert.equal(process.env.GITHUB_REF, 'refs/heads/main');
  const source = process.env.GITHUB_SHA;
  assert.match(source, /^[a-f0-9]{40}$/);
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
  git('fetch', 'origin', '--tags');
  const tags = git('tag', '--list', 'v*').split('\n').filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag));
  let tag = tags.find((candidate) => git('show', '-s', '--format=%B', `${candidate}^{commit}`).includes(`Release-Source: ${source}`));
  if (!tag) {
    const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
    const version = nextVersion(pkg.version, tags.map((value) => value.slice(1)));
    execFileSync(process.execPath, ['scripts/release-prepare.mjs', '--version', version], { stdio: 'inherit' });
    git('config', 'user.name', 'github-actions[bot]');
    git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com');
    git('add', 'package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', ...Object.keys(pkg.optionalDependencies).map((name) => `npm/${name}/package.json`));
    git('commit', '--allow-empty', '-m', `chore: release v${version}\n\nRelease-Source: ${source}`);
    tag = `v${version}`;
    git('tag', '-a', tag, '-m', `systemeame ${tag}`);
    git('push', 'origin', `refs/tags/${tag}`);
  }
  appendFileSync(process.env.GITHUB_OUTPUT, `tag=${tag}\nversion=${tag.slice(1)}\n`);
  console.log(`Reserved ${tag} for ${source}. Retries reuse the same release snapshot.`);
}
