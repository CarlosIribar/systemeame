import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';

// Execute npm's JavaScript entry point on Windows: .cmd launchers cannot be
// passed to execFileSync without a shell. npm run supplies npm_execpath.
export function runNpm(args, options) {
  const cli = process.env.npm_execpath;
  if (cli) return execFileSync(process.execPath, [cli, ...args], options);
  if (process.platform === 'win32') {
    return execFileSync(process.execPath, [join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js'), ...args], options);
  }
  return execFileSync('npm', args, options);
}
