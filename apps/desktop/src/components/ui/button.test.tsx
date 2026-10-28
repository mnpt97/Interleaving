import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { Button } from './button';
import { Popover, PopoverContent, PopoverTrigger } from './popover';

it('shows icon help on keyboard focus and dismisses it with Escape', async () => {
  render(
    <Button size="icon" aria-label="Save scenario" title="Save (⌘/Ctrl S)">
      S
    </Button>
  );
  fireEvent.focus(screen.getByRole('button', { name: 'Save scenario' }));
  expect(await screen.findByRole('tooltip')).toHaveTextContent('Save (⌘/Ctrl S)');
  fireEvent.keyDown(document, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('tooltip')).not.toBeInTheDocument());
});

it('keeps disabled icon help keyboard accessible without enabling the action', async () => {
  const action = vi.fn();
  render(
    <Button size="icon" aria-label="Undo" disabled onClick={action}>
      U
    </Button>
  );
  const button = screen.getByRole('button', { name: 'Undo' });
  expect(button).toBeDisabled();
  fireEvent.focus(button.parentElement!);
  expect(await screen.findByRole('tooltip')).toHaveTextContent('Undo');
  fireEvent.click(button);
  expect(action).not.toHaveBeenCalled();
});

it('preserves popover trigger composition for an icon button', async () => {
  render(
    <Popover>
      <PopoverTrigger asChild>
        <Button size="icon" aria-label="Add node">
          +
        </Button>
      </PopoverTrigger>
      <PopoverContent>Node choices</PopoverContent>
    </Popover>
  );
  fireEvent.click(screen.getByRole('button', { name: 'Add node' }));
  expect(await screen.findByRole('dialog')).toHaveTextContent('Node choices');
});
