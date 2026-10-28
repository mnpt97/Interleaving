import { expect, it } from 'vitest';
import { lifecycleRunModel } from './LifecycleRunCanvas';
import { begin } from './fixtures';
import { projectRun } from './projection';

it('stacks captured phases vertically while preserving horizontal workflow positions', () => {
  const projection = projectRun([begin()]);
  const model = lifecycleRunModel(projection, null, true, null, null);
  const frames = model.nodes.filter((node) => node.type === 'runPhase');
  expect(frames).toHaveLength(3);
  expect(new Set(frames.map((node) => node.position.x + (node.data.width as number) / 2)).size).toBe(1);
  expect(frames[1].data.width as number).toBeGreaterThan(frames[0].data.width as number);
  expect(frames[1].data.width as number).toBeGreaterThan(frames[2].data.width as number);
  expect(frames[1].position.y).toBeGreaterThan(frames[0].position.y + (frames[0].data.height as number));
  expect(frames[2].position.y).toBeGreaterThan(frames[1].position.y + (frames[1].data.height as number));
  const content = model.nodes.filter((node) => node.parentId === frames[1].id && !node.data.port);
  expect(content[1].position.x).toBeGreaterThan(content[0].position.x);
  expect(
    model.edges
      .filter((edge) => edge.id.startsWith('run-order:'))
      .every((edge) => edge.sourceHandle === 'bottom' && edge.targetHandle === 'top' && !edge.data?.motion)
  ).toBe(true);
  const start = model.nodes.find((node) => node.id === 'run:start')!;
  const end = model.nodes.find((node) => node.id === 'run:end')!;
  expect(start.position.x).toBe(end.position.x);
  expect(start.position.x).toBe(frames[0].position.x + (frames[0].data.width as number) / 2);
  expect(start.position.y).toBeLessThan(frames[0].position.y);
  expect(end.position.y).toBeGreaterThan(frames[2].position.y + (frames[2].data.height as number));
});
