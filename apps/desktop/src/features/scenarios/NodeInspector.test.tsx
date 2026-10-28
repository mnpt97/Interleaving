import { beforeEach, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import authSource from '../../../../../scenarios/examples/auth-login.interl.json?raw';
import { documentSchema } from '../../lib/scenario-types';
const authDocument = documentSchema.parse(JSON.parse(authSource));
import { NodeInspector } from './NodeInspector';
import { ValidationPanel } from './ValidationPanel';
import { exampleDocument, useScenarioStore } from './store';

beforeEach(() =>
  useScenarioStore.setState({
    document: structuredClone(exampleDocument),
    selectedId: 'health',
    diagnostics: [],
    status: 'valid',
    layoutDirty: false,
    mode: 'edit',
    inspectorTab: 'node',
    focusedFieldPath: null,
    formErrors: {},
  })
);

it('edits positions without changing workflow semantics', async () => {
  const workflow = useScenarioStore.getState().document!.workflows[0];
  render(<NodeInspector />);
  fireEvent.change(screen.getByLabelText('X'), { target: { value: '144' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply position' }));
  await waitFor(() => expect(useScenarioStore.getState().document?.workflows[0].layout?.positions.health.x).toBe(144));
  expect(useScenarioStore.getState().document!.workflows[0].nodes).toBe(workflow.nodes);
});

it('shows a recovery message for an invalid coordinate', async () => {
  render(<NodeInspector />);
  fireEvent.change(screen.getByLabelText('X'), { target: { value: '' } });
  fireEvent.click(screen.getByRole('button', { name: 'Apply position' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('finite numeric coordinates');
});

it('selects the relevant node from a diagnostic', () => {
  useScenarioStore.setState({
    selectedId: null,
    status: 'invalid',
    diagnostics: [
      {
        category: 'validation',
        code: 'invalidTimeout',
        message: 'Timeout must be positive.',
        fieldPath: '/workflows/0/nodes/1/config/timeoutMs',
        nodeId: 'health',
        filePath: null,
        line: null,
        column: null,
      },
    ],
  });
  render(<ValidationPanel />);
  fireEvent.click(screen.getByRole('button', { name: /Timeout must be positive/ }));
  expect(useScenarioStore.getState().selectedId).toBe('health');
});

it('inspects extraction without treating it as an exit point', () => {
  useScenarioStore.setState({ document: authDocument, selectedId: 'extract-token' });
  render(<NodeInspector />);
  expect(screen.getByLabelText('Extraction configuration')).toHaveTextContent('response.body.accessToken');
  expect(screen.getByLabelText('Extraction configuration')).toHaveTextContent('login');
  expect(screen.getByLabelText('Assigned identity references')).toHaveTextContent('INTERL_ALICE_PASSWORD');
  expect(screen.queryByText('Workflow exit point')).not.toBeInTheDocument();
});

it('shows session operations and references while hiding literal values and prefixes', () => {
  const document = structuredClone(authDocument);
  const node = document.workflows[0].nodes.find((item) => item.type === 'setSession')!;
  if (node.type !== 'setSession') throw new Error('Missing session fixture');
  node.config.setHeaders['x-private'] = {
    source: 'literal',
    value: 'private-literal-marker',
    prefix: 'private-prefix-marker',
  };
  node.config.removeHeaders = ['x-old-token'];
  useScenarioStore.setState({ document, selectedId: node.id });
  render(<NodeInspector />);
  expect(screen.getByLabelText('Session header operations')).toHaveTextContent('authorization');
  expect(screen.getByLabelText('Session header operations')).toHaveTextContent('extract-token.token');
  expect(screen.getByLabelText('Session header operations')).toHaveTextContent('x-old-token');
  expect(screen.queryByText(/private-literal-marker|private-prefix-marker/)).not.toBeInTheDocument();
});

it('inspects nested JSON template references in View while leaving literal content hidden', () => {
  const document = structuredClone(authDocument);
  const node = document.workflows[0].nodes[1];
  if (node.type !== 'http' || node.config.body?.kind !== 'jsonTemplate') throw new Error('Missing template fixture');
  node.config.body.fields.extra = {
    source: 'array',
    items: [
      { source: 'literal', value: 'private-body-marker' },
      { source: 'extraction', nodeId: 'prior', name: 'output' },
    ],
  };
  useScenarioStore.setState({ document, selectedId: 'login', mode: 'view' });
  render(<NodeInspector />);
  expect(screen.getByLabelText('Request value references')).toHaveTextContent('password: identity.password');
  expect(screen.getByLabelText('Request value references')).toHaveTextContent('extra[1]: prior.output');
  expect(screen.queryByText(/private-body-marker/)).not.toBeInTheDocument();
});

it('displays expression locations separately and opens the selected auth node inspector', () => {
  useScenarioStore.setState({
    document: authDocument,
    inspectorOpen: false,
    diagnostics: [
      {
        category: 'validation',
        code: 'invalidExpression',
        message: 'Unsupported expression',
        fieldPath: '/workflows/0/nodes/2/config/expression',
        nodeId: 'extract-token',
        filePath: null,
        line: 20,
        column: 4,
        expressionLine: 1,
        expressionColumn: 14,
      },
    ],
  });
  render(<ValidationPanel />);
  const diagnostic = screen.getByRole('button', { name: /Unsupported expression/ });
  expect(diagnostic).toHaveTextContent('expression line 1, column 14');
  expect(diagnostic).toHaveTextContent('line 20, column 4');
  fireEvent.click(diagnostic);
  expect(useScenarioStore.getState()).toMatchObject({ selectedId: 'extract-token', inspectorOpen: true });
});

it('does not select a different workflow node with the same local ID from a diagnostic', () => {
  useScenarioStore.setState({
    selectedId: 'health',
    diagnostics: [
      {
        category: 'validation',
        code: 'invalidRequestPath',
        message: 'Invalid path',
        fieldPath: '/workflows/1/nodes/1/config/path',
        workflowId: 'other',
        nodeId: 'health',
        filePath: null,
        line: null,
        column: null,
      },
    ],
  });
  render(<ValidationPanel />);
  fireEvent.click(screen.getByRole('button', { name: /Invalid path/ }));
  expect(useScenarioStore.getState().selectedId).toBeNull();
  expect(screen.getByText(/workflow other/)).toBeInTheDocument();
});
