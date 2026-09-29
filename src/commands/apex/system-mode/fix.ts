import { Flags } from '@oclif/core';
import { SfCommand } from '@salesforce/sf-plugins-core';
import { performance } from 'node:perf_hooks';
import { runNative } from '../../../native/run.js';

type FixResult = { result: unknown };

/** The Salesforce CLI boundary. Apex analysis is deliberately kept in Rust. */
export default class Fix extends SfCommand<FixResult> {
  public static readonly summary = 'Apply explicit system mode to eligible Apex operations.';
  public static readonly requiresProject = false;
  public static readonly enableJsonFlag = true;
  public static readonly flags = {
    all: Flags.boolean({ summary: 'Process all Apex files in package directories.' }),
    'project-dir': Flags.directory({ summary: 'Salesforce project directory.', default: process.cwd() }),
    'dry-run': Flags.boolean({ summary: 'Show changes without writing files.' }),
    check: Flags.boolean({ summary: 'Fail when eligible operations need changes.' }),
    'allow-unstaged': Flags.boolean({ summary: 'Allow staged files with working-tree changes.' }),
    quiet: Flags.boolean({ summary: 'Suppress non-error human output.' }),
    verbose: Flags.boolean({ summary: 'Show detailed file decisions.' }),
    jobs: Flags.integer({ summary: 'Maximum analysis workers.', min: 1 }),
  };

  public async run(): Promise<FixResult> {
    const startedAt = performance.now();
    const { flags } = await this.parse(Fix);
    if (flags.check && flags['dry-run']) this.error('--check cannot be used with --dry-run.');
    if (flags.quiet && flags.verbose) this.error('--quiet cannot be used with --verbose.');
    if (flags.all && flags['allow-unstaged']) this.error('--allow-unstaged cannot be used with --all.');
    // Scope discovery and filesystem writes are intentionally native. This request
    // shape is already versioned so the wrapper will not duplicate those rules.
    const finished = await runNative({ protocolVersion: 1, projectDir: flags['project-dir'], all: flags.all, dryRun: flags['dry-run'], check: flags.check, allowUnstaged: flags['allow-unstaged'] });
    const durationSeconds = (performance.now() - startedAt) / 1000;
    if (!this.jsonEnabled() && !flags.quiet) this.info(`systemeame finished with ${finished.result.proposedEdits} proposed edits in ${durationSeconds.toFixed(3)} seconds.`);
    if (!this.jsonEnabled()) {
      for (const diagnostic of finished.result.diagnostics) {
        this.warn(`[${diagnostic.code}] ${diagnostic.message}${diagnostic.suggestion ? ` ${diagnostic.suggestion}` : ''}`);
      }
    }
    // Keep the structured SfCommand result available (especially for --json),
    // while preserving the native command's documented logical status.
    if (finished.exitCode !== 0) process.exitCode = finished.exitCode;
    return { result: { ...finished.result, durationSeconds } };
  }
}
