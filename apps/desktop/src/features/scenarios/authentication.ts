import { extractionOutputs } from './extraction';
import { validBaseUrl } from './targets';
import type { ScenarioDocument, WorkflowDefinition, WorkflowNode } from '../../lib/scenario-types';
import { allocateId } from './ids';
export interface AuthenticationSettings {
  name: string;
  baseUrl?: string;
  path: string;
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  mode: 'bearer' | 'cookie';
  usernameEnv: string;
  passwordEnv: string;
  usernameKey: string;
  passwordKey: string;
  tokenExpression: string;
  headerName: string;
  headerPrefix: string;
}
export function authenticationDefinition(document: ScenarioDocument, node: WorkflowNode): WorkflowDefinition | null {
  if (
    node.type !== 'call' ||
    Object.keys(node.config.inputs).sort().join(',') !== 'password,username' ||
    Object.values(node.config.inputs).some((v) => v.source !== 'identity')
  )
    return null;
  const flow = document.workflows.find((w) => w.id === node.config.workflowId);
  if (
    !flow ||
    Object.keys(flow.inputs).sort().join(',') !== 'password,username' ||
    Object.values(flow.inputs).some((v) => v.type !== 'string' || v.default !== undefined) ||
    Object.keys(flow.outputs).length
  )
    return null;
  const login = flow.nodes.find((n) => n.type === 'http');
  if (
    !login ||
    login.type !== 'http' ||
    login.config.body?.kind !== 'jsonTemplate' ||
    Object.keys(login.config.headers).length ||
    login.config.timeoutMs != null
  )
    return null;
  const fields = Object.values(login.config.body.fields);
  if (
    fields.length !== 2 ||
    !fields.some((f) => f.source === 'input' && f.name === 'username') ||
    !fields.some((f) => f.source === 'input' && f.name === 'password')
  )
    return null;
  const start = flow.nodes.find((n) => n.type === 'start');
  const end = flow.nodes.find((n) => n.type === 'end');
  const extract = flow.nodes.find((n) => n.type === 'extract');
  const session = flow.nodes.find((n) => n.type === 'setSession');
  if (!start || !end || !['POST', 'PUT', 'PATCH'].includes(login.config.method)) return null;
  let order = [start.id, login.id, end.id];
  if (extract && session && extract.type === 'extract' && session.type === 'setSession') {
    const headers = Object.values(session.config.setHeaders);
    if (extract.config.sourceNodeId !== login.id || session.config.removeHeaders.length || headers.length !== 1)
      return null;
    const header = headers[0];
    const outputs = Object.keys(extractionOutputs(extract.config));
    if (outputs.length !== 1) return null;
    if (header.source !== 'extraction' || header.nodeId !== extract.id || header.name !== outputs[0]) return null;
    order = [start.id, login.id, extract.id, session.id, end.id];
  } else if (extract || session) return null;
  if (flow.nodes.length !== order.length || flow.edges.length !== order.length - 1) return null;
  if (
    order
      .slice(1)
      .some(
        (to, i) =>
          !flow.edges.some((e) => e.from === order[i] && e.to === to && !e.condition && e.default === undefined)
      )
  )
    return null;
  return flow;
}
export function readAuthentication(
  document: ScenarioDocument,
  node: Extract<WorkflowNode, { type: 'call' }>,
  flow: WorkflowDefinition
): AuthenticationSettings {
  const login = flow.nodes.find((n) => n.type === 'http')! as Extract<WorkflowNode, { type: 'http' }>;
  const fields = login.config.body?.kind === 'jsonTemplate' ? Object.entries(login.config.body.fields) : [];
  const username = fields.find(([, v]) => v.source === 'input' && v.name === 'username')!;
  const password = fields.find(([, v]) => v.source === 'input' && v.name === 'password')!;
  const usernameField =
    node.config.inputs.username.source === 'identity' ? node.config.inputs.username.field : 'username';
  const passwordField =
    node.config.inputs.password.source === 'identity' ? node.config.inputs.password.field : 'password';
  const identityId = document.groups[0]?.identityAssignment.identityId ?? '';
  const identity = document.identities.find((i) => i.id === identityId);
  const extract = flow.nodes.find((n) => n.type === 'extract');
  const session = flow.nodes.find((n) => n.type === 'setSession');
  const header = session?.type === 'setSession' ? Object.entries(session.config.setHeaders)[0] : undefined;
  return {
    name: node.name ?? flow.name,
    path: login.config.path,
    ...(login.config.baseUrl ? { baseUrl: login.config.baseUrl } : {}),
    method: login.config.method as AuthenticationSettings['method'],
    mode: extract ? 'bearer' : 'cookie',
    usernameEnv: identity?.credentials[usernameField]?.name ?? '',
    passwordEnv: identity?.credentials[passwordField]?.name ?? '',
    usernameKey: username[0],
    passwordKey: password[0],
    tokenExpression:
      extract?.type === 'extract'
        ? (Object.values(extractionOutputs(extract.config))[0]?.expression ?? 'response.body.accessToken')
        : 'response.body.accessToken',
    headerName: header?.[0] ?? 'Authorization',
    headerPrefix: header?.[1].prefix ?? 'Bearer ',
  };
}
export function authenticationErrors(s: AuthenticationSettings): string[] {
  const errors: string[] = [];
  if (s.baseUrl && !validBaseUrl(s.baseUrl))
    errors.push('Enter an absolute HTTP(S) base URL without credentials, query or fragment.');
  if (new TextEncoder().encode(s.name).length > 256) errors.push('Enter a name of at most 256 UTF-8 bytes.');
  if (!['POST', 'PUT', 'PATCH'].includes(s.method)) errors.push('Use POST, PUT, or PATCH for authentication.');
  if (!s.path.startsWith('/') || s.path.startsWith('//') || /[\r\n]/.test(s.path))
    errors.push('Enter a relative login path, for example /auth/login.');
  const identifier = /^[A-Za-z_][A-Za-z0-9_]{0,127}$/;
  if (![s.usernameEnv, s.passwordEnv, s.usernameKey, s.passwordKey].every((v) => identifier.test(v)))
    errors.push('JSON keys and environment variable names must be ASCII identifiers of at most 128 characters.');
  if (s.usernameKey === s.passwordKey) errors.push('Username and password must use different JSON keys.');
  if (s.mode === 'bearer') {
    if (!s.tokenExpression.trim()) errors.push('Enter the response token expression.');
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(s.headerName)) errors.push('Enter a valid session header name.');
    if (/[\r\n]/.test(s.headerPrefix)) errors.push('Header prefixes cannot contain line breaks.');
  }
  return errors;
}
export function configureAuthentication(
  document: ScenarioDocument,
  parentId: string,
  nodeId: string,
  settings: AuthenticationSettings
): void {
  const errors = authenticationErrors(settings);
  if (errors.length) throw new Error(errors.join(' '));
  const parent = document.workflows.find((w) => w.id === parentId);
  const index = parent?.nodes.findIndex((n) => n.id === nodeId) ?? -1;
  const previous = parent?.nodes[index];
  if (!parent || !previous || (previous.type !== 'authentication' && !authenticationDefinition(document, previous)))
    throw new Error('Authentication node is missing.');
  parent.nodes[index] = {
    id: nodeId,
    type: 'authentication',
    name: settings.name,
    config: {
      method: settings.method,
      path: settings.path,
      ...(settings.baseUrl ? { baseUrl: settings.baseUrl } : {}),
      ...(settings.mode === 'bearer'
        ? {
            mode: 'bearer' as const,
            tokenExpression: settings.tokenExpression,
            headerName: settings.headerName,
            headerPrefix: settings.headerPrefix,
          }
        : { mode: 'cookie' as const }),
      username: { source: 'env', name: settings.usernameEnv },
      password: { source: 'env', name: settings.passwordEnv },
      usernameKey: settings.usernameKey,
      passwordKey: settings.passwordKey,
    },
  };
  if (previous.type === 'call') {
    const id = previous.config.workflowId;
    const referenced =
      document.workflows.some((w) => w.nodes.some((n) => n.type === 'call' && n.config.workflowId === id)) ||
      document.groups.some((g) =>
        [g.lifecycle.initialize, g.lifecycle.iteration, g.lifecycle.cleanup].some((b) => b?.workflowId === id)
      );
    if (!referenced) document.workflows = document.workflows.filter((w) => w.id !== id);
  }
}
export function insertAuthentication(document: ScenarioDocument, workflowId: string, edgeId?: string): void {
  const parent = document.workflows.find((w) => w.id === workflowId);
  if (!parent) throw new Error('Select a workflow first.');
  const incoming = parent.edges.filter((e) => parent.nodes.some((n) => n.id === e.from && n.type === 'start'));
  const edge = edgeId ? parent.edges.find((e) => e.id === edgeId) : incoming.length === 1 ? incoming[0] : undefined;
  if (!edge || edge.condition || edge.default !== undefined) throw new Error('Choose an insertion connection.');
  const callId = allocateId(
    'authenticate',
    parent.nodes.map((n) => n.id)
  );
  parent.nodes.splice(
    parent.nodes.findIndex((n) => n.id === edge.to),
    0,
    {
      id: callId,
      type: 'authentication',
      name: 'Authentication',
      config: {
        method: 'POST',
        path: '/auth/login',
        mode: 'bearer',
        username: { source: 'env', name: 'INTERL_USERNAME' },
        password: { source: 'env', name: 'INTERL_PASSWORD' },
        usernameKey: 'username',
        passwordKey: 'password',
        tokenExpression: 'response.body.accessToken',
        headerName: 'Authorization',
        headerPrefix: 'Bearer ',
      },
    }
  );
  parent.edges = parent.edges.filter((e) => e !== edge);
  parent.edges.push({
    id: allocateId(
      'edge',
      parent.edges.map((e) => e.id)
    ),
    from: edge.from,
    to: callId,
  });
  parent.edges.push({
    id: allocateId(
      'edge',
      parent.edges.map((e) => e.id)
    ),
    from: callId,
    to: edge.to,
  });
  const positions = { ...parent.layout?.positions };
  const previous = positions[edge.from] ?? { x: 80, y: 160 };
  // Make room along the explicitly connected continuation, not by array order.
  const downstream = [edge.to];
  const visited = new Set<string>();
  while (downstream.length) {
    const id = downstream.pop()!;
    if (visited.has(id) || id === callId) continue;
    visited.add(id);
    if (positions[id]) positions[id] = { ...positions[id], x: positions[id].x + 260 };
    downstream.push(...parent.edges.filter((e) => e.from === id).map((e) => e.to));
  }
  positions[callId] = { x: previous.x + 260, y: previous.y };
  positions[edge.to] = { x: previous.x + 520, y: previous.y };
  parent.layout = { positions };
}

export function readNativeAuthentication(
  node: Extract<WorkflowNode, { type: 'authentication' }>
): AuthenticationSettings {
  return {
    ...node.config,
    name: node.name ?? '',
    tokenExpression: node.config.mode === 'bearer' ? node.config.tokenExpression : 'response.body.accessToken',
    headerName: node.config.mode === 'bearer' ? node.config.headerName : 'Authorization',
    headerPrefix: node.config.mode === 'bearer' ? node.config.headerPrefix : 'Bearer ',
    usernameEnv: node.config.username.name,
    passwordEnv: node.config.password.name,
  };
}
