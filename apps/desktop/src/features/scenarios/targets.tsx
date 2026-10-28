import { useEffect, useState } from 'react';
import type { ScenarioDocument, WorkflowNode } from '../../lib/scenario-types';
import { useScenarioStore } from './store';

export function validBaseUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (
      value.length <= 8192 &&
      ['http:', 'https:'].includes(url.protocol) &&
      Boolean(url.hostname) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash
    );
  } catch {
    return false;
  }
}
export function scenarioBaseUrl(document: ScenarioDocument): string {
  const group = document.groups[0];
  return document.clientProfiles.find((p) => p.id === group?.clientProfileId)?.baseUrl ?? '';
}
export function effectiveUrl(document: ScenarioDocument, override: string | undefined, path: string): string {
  const base = override || scenarioBaseUrl(document);
  try {
    if (!validBaseUrl(base)) return 'Enter a valid base URL';
    const url = new URL(base);
    url.pathname = `${url.pathname.replace(/\/$/, '')}/`;
    const result = new URL(path.replace(/^\//, ''), url);
    return result.origin === url.origin && result.pathname.startsWith(url.pathname)
      ? result.toString()
      : 'Path must stay within the base URL prefix';
  } catch {
    return 'Enter a valid request path';
  }
}
export function ScenarioBaseUrl() {
  const document = useScenarioStore((s) => s.document);
  const editing = useScenarioStore((s) => s.mode === 'edit');
  const value = document ? scenarioBaseUrl(document) : '';
  return document ? (
    <section className="sidebar-section scenario-target">
      <h2>Scenario base URL</h2>
      {editing ? (
        <BaseUrlInput
          key="scenario"
          value={value}
          errorKey="scenario:baseUrl"
          onChange={(baseUrl) => useScenarioStore.getState().command({ type: 'baseUrl', baseUrl }, 'scenario:baseUrl')}
        />
      ) : (
        <code>{value}</code>
      )}
    </section>
  ) : null;
}
function BaseUrlInput({
  value,
  errorKey,
  placeholder,
  optional = false,
  onChange,
}: {
  value: string;
  errorKey: string;
  placeholder?: string;
  optional?: boolean;
  onChange(value: string): void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  const valid = (optional && !draft) || validBaseUrl(draft);
  useEffect(() => {
    useScenarioStore.getState().setFormError(errorKey, valid ? null : 'Enter an absolute HTTP(S) base URL.');
    return () => useScenarioStore.getState().setFormError(errorKey, null);
  }, [errorKey, valid]);
  return (
    <label>
      Base URL
      <input
        value={draft}
        placeholder={placeholder}
        aria-invalid={!valid}
        onChange={(event) => {
          const next = event.target.value;
          setDraft(next);
          if ((optional && !next) || validBaseUrl(next)) onChange(next);
        }}
        onBlur={() => useScenarioStore.getState().endHistoryGroup()}
      />
      {!valid && (
        <span className="field-error" role="alert">
          Use an absolute HTTP(S) URL without credentials, query or fragment.
        </span>
      )}
    </label>
  );
}
export function NodeBaseUrl({
  workflowId,
  node,
}: {
  workflowId: string;
  node: Extract<WorkflowNode, { type: 'http' }>;
}) {
  const document = useScenarioStore((s) => s.document)!;
  return (
    <>
      <BaseUrlInput
        value={node.config.baseUrl ?? ''}
        errorKey={`baseUrl:${workflowId}:${node.id}`}
        optional
        placeholder={scenarioBaseUrl(document)}
        onChange={(value) => {
          const config = { ...node.config };
          if (value) config.baseUrl = value;
          else delete config.baseUrl;
          useScenarioStore
            .getState()
            .command({ type: 'node.put', workflowId, node: { ...node, config } }, `baseUrl:${workflowId}:${node.id}`);
        }}
      />
    </>
  );
}
