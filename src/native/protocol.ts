export const PROTOCOL_VERSION = 1;

export type NativeRequest = { protocolVersion: typeof PROTOCOL_VERSION; source?: string; projectDir?: string; all?: boolean; dryRun?: boolean; check?: boolean; allowUnstaged?: boolean };
export type NativeDiagnostic = { code: string; message: string; suggestion?: string };
export type NativeFinished = { protocolVersion: number; event: 'finished'; exitCode: number; result: { schemaVersion: number; diagnostics: NativeDiagnostic[]; proposedEdits: number } };

export function assertFinished(value: unknown): asserts value is NativeFinished {
  if (!value || typeof value !== 'object') throw new Error('Native process emitted invalid JSON.');
  const event = value as Partial<NativeFinished>;
  if (event.protocolVersion !== PROTOCOL_VERSION || event.event !== 'finished' || typeof event.exitCode !== 'number') throw new Error('Native process emitted an incompatible protocol event.');
}
