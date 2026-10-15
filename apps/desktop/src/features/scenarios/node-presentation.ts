import { CirclePlay, Flag, Globe, KeyRound, ScanText } from 'lucide-react';
import type { WorkflowNode } from '../../lib/scenario-types';

export const nodePresentation = {
  start: { label: 'Start', description: 'Workflow entry point', Icon: CirclePlay },
  http: { label: 'HTTP request', description: 'HTTP request', Icon: Globe },
  extract: { label: 'Extract', description: 'Read-only response extraction', Icon: ScanText },
  setSession: { label: 'Set session', description: "Update this user's session headers", Icon: KeyRound },
  call: { label: 'Call workflow', description: 'Invoke a reusable workflow in this user session', Icon: Globe },
  exclusiveSplit: { label: 'Exclusive split', description: 'Routing is available in a later phase', Icon: Globe },
  exclusiveMerge: { label: 'Exclusive merge', description: 'Routing is available in a later phase', Icon: Globe },
  end: { label: 'End', description: 'Workflow exit point', Icon: Flag },
} satisfies Record<WorkflowNode['type'], { label: string; description: string; Icon: typeof Globe }>;
