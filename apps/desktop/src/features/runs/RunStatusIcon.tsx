import {
  CircleCheck,
  CircleDashed,
  CircleHelp,
  CircleMinus,
  CirclePlay,
  CircleSlash,
  CircleX,
  Minus,
} from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../components/ui/tooltip';
import type { Progress } from './projection';

const presentation = {
  active: { Icon: CirclePlay, label: 'Active' },
  completed: { Icon: CircleCheck, label: 'Completed' },
  skipped: { Icon: CircleMinus, label: 'Skipped' },
  failed: { Icon: CircleX, label: 'Failed' },
  cancelled: { Icon: CircleSlash, label: 'Cancelled' },
  pending: { Icon: CircleDashed, label: 'Pending' },
  unknown: { Icon: CircleHelp, label: 'Unknown' },
  'not configured': { Icon: Minus, label: 'Not configured' },
};

export function RunStatusIcon({ status }: { status: Progress | 'not configured' }) {
  const { Icon, label } = presentation[status];
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="run-status-icon" data-status={status} role="img" aria-label={label}>
          <Icon size={16} aria-hidden="true" />
        </span>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
