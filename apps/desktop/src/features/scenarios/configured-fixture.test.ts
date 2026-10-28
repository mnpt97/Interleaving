import configuredSource from '../../../../../crates/interl-engine/tests/fixtures/editor-configured.json?raw';
import { expect, it } from 'vitest';
import { executeCommand, newDraft } from './draft';
import { readNativeAuthentication } from './authentication';
import { documentSchema } from '../../lib/scenario-types';

it('reproduces a portable configured lifecycle/template/call fixture through editor commands', () => {
  let document = newDraft();
  document.id = 'editor-configured';
  document = executeCommand(document, { type: 'name', name: 'Configured editor workflow' });
  document = executeCommand(document, { type: 'workflow.create', name: 'Initialize' });
  document = executeCommand(document, { type: 'authentication.insert', workflowId: 'workflow-2' });
  const auth = document.workflows[1].nodes[1];
  if (auth.type !== 'authentication') throw new Error('Authentication');
  document = executeCommand(document, {
    type: 'authentication.configure',
    workflowId: 'workflow-2',
    nodeId: auth.id,
    settings: {
      ...readNativeAuthentication(auth),
      mode: 'cookie',
      usernameEnv: 'INTERL_ALICE_USERNAME',
      passwordEnv: 'INTERL_ALICE_PASSWORD',
    },
  });
  document = executeCommand(document, { type: 'workflow.create', name: 'Forward status' });
  document = executeCommand(document, {
    type: 'workflow.configure',
    workflowId: 'workflow-3',
    configuration: {
      name: 'Forward status',
      inputs: { status: { type: 'integer' } },
      outputs: { result: { type: 'integer', value: { source: 'input', name: 'status' } } },
    },
  });
  document = executeCommand(document, { type: 'http.insert', workflowId: 'workflow-1' });
  const http = document.workflows[0].nodes[1];
  if (http.type !== 'http') throw new Error('HTTP');
  document = executeCommand(document, {
    type: 'node.put',
    workflowId: 'workflow-1',
    node: {
      ...http,
      config: {
        method: 'POST',
        path: '/echo?resource=${resourceId}',
        headers: { 'X-Editor': 'configured' },
        timeoutMs: 2000,
        body: {
          kind: 'jsonTemplate',
          fields: {
            resourceId: { source: 'input', name: 'resourceId' },
            metadata: { source: 'object', fields: { active: { source: 'literal', value: true } } },
            values: {
              source: 'array',
              items: [
                { source: 'literal', value: 1 },
                { source: 'literal', value: 'two' },
              ],
            },
          },
        },
      },
    },
  });
  document = executeCommand(document, {
    type: 'node.insert',
    workflowId: 'workflow-1',
    kind: 'call',
    calleeId: 'workflow-3',
    afterNodeId: 'http-1',
  });
  const call = document.workflows[0].nodes.find((node) => node.type === 'call');
  if (call?.type !== 'call') throw new Error('Call');
  document = executeCommand(document, {
    type: 'node.put',
    workflowId: 'workflow-1',
    node: {
      ...call,
      config: { ...call.config, inputs: { status: { source: 'response', nodeId: 'http-1', field: 'status' } } },
    },
  });
  document = executeCommand(document, {
    type: 'workflow.configure',
    workflowId: 'workflow-1',
    configuration: {
      name: 'Iteration',
      inputs: { resourceId: { type: 'integer', default: 1 } },
      outputs: { status: { type: 'integer', value: { source: 'callOutput', nodeId: call.id, name: 'result' } } },
    },
  });
  document = executeCommand(document, { type: 'workflow.create', name: 'Cleanup' });
  document = executeCommand(document, { type: 'node.insert', workflowId: 'workflow-4', kind: 'setSession' });
  const cleanup = document.workflows[3].nodes[1];
  if (cleanup.type !== 'setSession') throw new Error('Session');
  document = executeCommand(document, {
    type: 'node.put',
    workflowId: 'workflow-4',
    node: { ...cleanup, config: { setHeaders: {}, removeHeaders: ['Authorization'] } },
  });
  document = executeCommand(document, {
    type: 'scenario.configure',
    configuration: {
      environment: { variables: { resourceId: 1 } },
      clientProfiles: [{ ...document.clientProfiles[0], timeoutMs: 7000, headers: { 'X-Client': 'editor' } }],
      identities: [{ id: 'unused', credentials: { password: { source: 'env', name: 'UNUSED_PASSWORD' } } }],
      groups: [
        {
          ...document.groups[0],
          lifecycle: {
            initialize: { workflowId: 'workflow-2', inputs: {} },
            iteration: {
              workflowId: 'workflow-1',
              inputs: { resourceId: { source: 'environment', name: 'resourceId' } },
            },
            cleanup: { workflowId: 'workflow-4', inputs: {} },
            cleanupTimeoutMs: 4000,
          },
        },
      ],
    },
  });
  expect(document).toEqual(documentSchema.parse(JSON.parse(configuredSource)));
  expect(JSON.stringify(document)).toContain('INTERL_ALICE_PASSWORD');
  expect(JSON.stringify(document)).not.toContain('alice-password');
});
