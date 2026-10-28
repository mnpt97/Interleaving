import { effectiveUrl, scenarioBaseUrl } from './targets';
import { useEffect, useState } from 'react';
import type { ScenarioDocument, WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { Button } from '../../components/ui/button';
import {
  authenticationErrors,
  readAuthentication,
  readNativeAuthentication,
  type AuthenticationSettings,
} from './authentication';
import { useScenarioStore } from './store';
export function AuthenticationEditor({
  document,
  node,
  workflowId,
  definition,
  editing,
  showName = true,
}: {
  document: ScenarioDocument;
  node: Extract<WorkflowNode, { type: 'call' | 'authentication' }>;
  workflowId: string;
  definition?: WorkflowDefinition;
  editing: boolean;
  showName?: boolean;
}) {
  const baseline = JSON.stringify(
    node.type === 'authentication' ? readNativeAuthentication(node) : readAuthentication(document, node, definition!)
  );
  const [values, setValues] = useState<AuthenticationSettings>(() => JSON.parse(baseline));
  useEffect(() => {
    setValues(JSON.parse(baseline));
  }, [baseline]);
  const dirty = JSON.stringify(values) !== baseline;
  const errors = authenticationErrors(values);
  const errorKey = `authentication:${workflowId}:${node.id}`;
  useEffect(() => {
    useScenarioStore
      .getState()
      .setFormError(errorKey, editing && dirty ? 'Apply the authentication changes before saving.' : null);
    return () => useScenarioStore.getState().setFormError(errorKey, null);
  }, [editing, dirty, errorKey]);
  function update<K extends keyof AuthenticationSettings>(key: K, value: AuthenticationSettings[K]) {
    setValues((old) => ({ ...old, [key]: value }));
  }
  if (!editing)
    return (
      <section className="auth-details" aria-label="Authentication configuration">
        <h3>Authentication</h3>
        <dl className="details">
          <div>
            <dt>Login</dt>
            <dd>
              {values.method} <code>{values.path}</code>
            </dd>
          </div>
          <div>
            <dt>Session</dt>
            <dd>{values.mode === 'bearer' ? 'Extract token and set session header' : 'Retain response cookies'}</dd>
          </div>
          <div>
            <dt>Effective URL</dt>
            <dd>
              <code>{effectiveUrl(document, values.baseUrl, values.path)}</code>
            </dd>
          </div>
        </dl>
      </section>
    );
  const fieldPath = `/workflows/${document.workflows.findIndex((workflow) => workflow.id === workflowId)}/nodes/${document.workflows.find((workflow) => workflow.id === workflowId)?.nodes.findIndex((candidate) => candidate.id === node.id)}`;
  const field = (key: keyof AuthenticationSettings, label: string, placeholder?: string) => (
    <label>
      {label}
      <input
        data-field-path={
          key === 'name'
            ? `${fieldPath}/name`
            : `${fieldPath}/config/${key === 'usernameEnv' ? 'username/name' : key === 'passwordEnv' ? 'password/name' : key}`
        }
        value={values[key] ?? ''}
        placeholder={placeholder}
        onChange={(e) => update(key, e.target.value)}
      />
    </label>
  );
  return (
    <form
      className="http-editor authentication-editor"
      aria-label="Authentication configuration"
      onSubmit={(e) => {
        e.preventDefault();
        if (errors.length) return;
        useScenarioStore
          .getState()
          .command({ type: 'authentication.configure', workflowId, nodeId: node.id, settings: values });
      }}
    >
      <h3>Login request</h3>
      {showName && field('name', 'Name', node.id)}
      <label>
        Method
        <select
          value={values.method}
          onChange={(e) => update('method', e.target.value as AuthenticationSettings['method'])}
        >
          {['POST', 'PUT', 'PATCH'].map((method) => (
            <option key={method}>{method}</option>
          ))}
        </select>
      </label>
      {field('baseUrl', 'Base URL override', scenarioBaseUrl(document))}

      {field('path', 'Login path', '/auth/login')}
      <p className="muted">
        Effective URL: <code>{effectiveUrl(document, values.baseUrl, values.path)}</code>
      </p>
      <h3>Credential references</h3>

      {field('usernameEnv', 'Username environment variable', 'INTERL_USERNAME')}
      {field('passwordEnv', 'Password environment variable', 'INTERL_PASSWORD')}
      <details>
        <summary>Request field mapping</summary>
        {field('usernameKey', 'Username JSON field', 'username')}
        {field('passwordKey', 'Password JSON field', 'password')}
      </details>
      <h3>Authenticated session</h3>
      <label>
        Authentication mode
        <select value={values.mode} onChange={(e) => update('mode', e.target.value as AuthenticationSettings['mode'])}>
          <option value="bearer">Bearer token</option>
          <option value="cookie">Session cookie</option>
        </select>
      </label>
      {values.mode === 'bearer' ? (
        <>
          {field('tokenExpression', 'Token expression', 'response.body.accessToken')}

          {field('headerName', 'Session header', 'Authorization')}
          {field('headerPrefix', 'Header prefix', 'Bearer ')}
        </>
      ) : null}

      {dirty && errors.length > 0 && (
        <ul className="field-error" role="alert">
          {errors.map((error) => (
            <li key={error}>{error}</li>
          ))}
        </ul>
      )}
      <Button type="submit" disabled={!dirty || errors.length > 0}>
        Apply authentication
      </Button>
    </form>
  );
}
