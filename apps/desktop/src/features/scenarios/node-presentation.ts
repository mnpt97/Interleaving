import { CirclePlay, Flag, Globe, KeyRound, ScanText } from 'lucide-react';
import type { WorkflowNode } from '../../lib/scenario-types';

export const nodePresentation = {
  start: { label: 'Start', description: 'Workflow entry point', Icon: CirclePlay },
  http: { label: 'HTTP request', description: 'HTTP request', Icon: Globe },
  extract: { label: 'Extract', description: 'Read-only response extraction', Icon: ScanText },
  setSession: { label: 'Set session', description: "Update this user's session headers", Icon: KeyRound },
  end: { label: 'End', description: 'Workflow exit point', Icon: Flag },
} satisfies Record<WorkflowNode['type'], { label: string; description: string; Icon: typeof Globe }>;
