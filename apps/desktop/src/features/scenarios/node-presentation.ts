import { CirclePlay, Flag, Globe, Workflow, KeyRound, ScanText, GitBranch, GitMerge } from 'lucide-react';
import { authenticationDefinition } from './authentication';
import type { ScenarioDocument, WorkflowNode } from '../../lib/scenario-types';

export const nodePresentation = {
  start: { label: 'Start', description: 'Workflow entry point', Icon: CirclePlay },
  authentication: {
    label: 'Authentication',
    description: 'Authenticate following nodes in this execution path',
    Icon: KeyRound,
  },
  http: { label: 'HTTP request', description: 'HTTP request', Icon: Globe },
  extract: { label: 'Extract', description: 'Read-only response extraction', Icon: ScanText },
  setSession: { label: 'Set session', description: "Update this user's session headers", Icon: KeyRound },
  call: { label: 'Call workflow', description: 'Invoke a reusable workflow in this user session', Icon: Workflow },
  exclusiveSplit: { label: 'Exclusive split', description: 'Choose exactly one conditional route', Icon: GitBranch },
  exclusiveMerge: { label: 'Exclusive merge', description: 'Rejoin the selected route', Icon: GitMerge },
  end: { label: 'End', description: 'Workflow exit point', Icon: Flag },
} satisfies Record<WorkflowNode['type'], { label: string; description: string; Icon: typeof Globe }>;

export function presentationFor(node: WorkflowNode, document: ScenarioDocument | null) {
  return document && authenticationDefinition(document, node)
    ? { label: 'Authentication', description: 'Log in and retain the calling user’s session', Icon: KeyRound }
    : nodePresentation[node.type];
}

export function nodeName(node: WorkflowNode, document?: ScenarioDocument | null): string {
  const authentication = document ? authenticationDefinition(document, node) : null;
  if (node.name?.trim()) return node.name;
  if (authentication) return authentication.name;
  return node.name?.trim() ? node.name : node.id;
}
