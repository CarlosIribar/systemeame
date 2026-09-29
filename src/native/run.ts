import { spawn } from 'node:child_process';
import { resolveNativeBinary } from './resolve.js';
import { assertFinished, type NativeFinished, type NativeRequest } from './protocol.js';

export async function runNative(request: NativeRequest): Promise<NativeFinished> {
  const binary = resolveNativeBinary();
  return new Promise((resolve, reject) => {
    const child = spawn(binary, [], { shell: false, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = ''; let stderr = '';
    child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => { stdout += chunk; });
    child.stderr.on('data', (chunk: string) => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', (code) => {
      try {
        const lines = stdout.trim().split('\n').filter(Boolean);
        if (lines.length !== 1) throw new Error('Native process did not emit exactly one finished event.');
        const result: unknown = JSON.parse(lines[0]); assertFinished(result);
        if (code !== result.exitCode) throw new Error(`Native exit-code mismatch (${code} != ${result.exitCode}).`);
        resolve(result);
      } catch (error) { reject(new Error(`${error instanceof Error ? error.message : String(error)}${stderr ? ` ${stderr.trim()}` : ''}`)); }
    });
    child.stdin.end(JSON.stringify(request));
  });
}
