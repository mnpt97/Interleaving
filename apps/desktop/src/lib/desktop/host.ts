import type { RunMessage } from '../run-types';
import type { OpenScenarioResult, ValidationResult, ScenarioFormat } from '../scenario-types';

/** Consumer-owned port: React never depends on either native SDK or generated service models. */
export interface DesktopHost {
  kind: 'wails' | 'browser';
  readonly native: boolean;
  /** Authoritative scenario services exist, rather than only a native shell. */
  readonly available: boolean;
  startScenario(
    source: string,
    receive: (message: RunMessage) => void
  ): Promise<{ runId: string; protocolVersion: number }>;
  cancelScenario(runId: string): Promise<boolean>;
  saveRunReport(runId: string): Promise<boolean>;
  validateSource(source: string, format: ScenarioFormat): Promise<ValidationResult>;
  openScenarioFile(): Promise<OpenScenarioResult | null>;
  readScenarioFile(path: string): Promise<OpenScenarioResult>;
  saveScenarioJson(
    source: string,
    id: string,
    destination?: string | null
  ): Promise<false | { path: string; validation: ValidationResult }>;
  setTheme(theme: 'dark' | 'light' | null): Promise<void>;
  onCloseRequested(allow: () => Promise<boolean>): Promise<() => void>;
}
export function unavailable(): never {
  throw {
    category: 'validation',
    code: 'unavailable',
    message: 'Go scenario services are not implemented yet.',
    fieldPath: '$',
    nodeId: null,
    filePath: null,
    line: null,
    column: null,
  };
}
