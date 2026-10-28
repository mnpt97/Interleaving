import { describe, expect, it } from 'vitest';
import { detailEvent, duration, nodeProgress, projectRun } from './projection';
import { begin, completeReport, observation as event, response } from './fixtures';

describe('execution projection', () => {
  it('combines node events without treating HTTP errors as execution failures', () => {
    const received = response(4);
    if (received.type !== 'response.received') throw new Error('fixture');
    received.payload.status = 404;
    const projection = projectRun([
      begin(),
      event('step.started', 2, { kind: 'http' }),
      event('request.sent', 3, {
        method: 'GET',
        url: 'http://local/health',
        headers: {},
        body: { value: null, bytes: 0, truncated: false, omitted: false },
      }),
      received,
      event('step.finished', 5),
    ]);
    const nodes = projection.invocations[0].nodes;
    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({ state: 'completed', httpStatus: 404 });
    expect(duration(nodes[0])).toBe('3.0 ms');
    expect(detailEvent(nodes[0])?.type).toBe('response.received');
    expect(detailEvent(nodes[0], 2)?.type).toBe('step.started');
    expect(nodes[0].events.map((event) => event.type)).toEqual([
      'step.started',
      'request.sent',
      'response.received',
      'step.finished',
    ]);
  });
  it('scopes identical local IDs and repeated calls by full invocation identity', () => {
    const projection = projectRun([
      begin(),
      event('step.started', 2, { kind: 'http' }),
      event('step.finished', 3),
      event(
        'step.started',
        4,
        { kind: 'http' },
        { workflowKey: 'child', invocationId: 'child1', parentInvocationId: 'root', callerNodeKey: 'a', nodeKey: 'ch' }
      ),
      event(
        'step.finished',
        5,
        {},
        { workflowKey: 'child', invocationId: 'child1', parentInvocationId: 'root', callerNodeKey: 'a', nodeKey: 'ch' }
      ),
      event(
        'step.started',
        6,
        { kind: 'http' },
        { workflowKey: 'child', invocationId: 'child2', parentInvocationId: 'root', callerNodeKey: 'a', nodeKey: 'ch' }
      ),
      event('step.started', 7, { kind: 'http' }),
      event('step.finished', 8),
    ]);
    expect(projection.invocations).toHaveLength(3);
    expect(projection.invocations[0].nodes).toHaveLength(2);
    expect(new Set(projection.invocations.flatMap((invocation) => invocation.nodes.map((node) => node.key))).size).toBe(
      4
    );
    expect(projection.invocations[2].nodes[0].state).toBe('active');
  });
  it('uses only explicit traversal and route observations to mark the selected and skipped paths', () => {
    const projection = projectRun([
      begin(),
      event('step.started', 2, { kind: 'exclusiveSplit' }, { nodeKey: 'x' }, 'choice'),
      event('route.selected', 3, { selectedEdgeKey: 'xa', unselectedEdgeKeys: ['xb'] }, { nodeKey: 'x' }, 'choice'),
      event('edge.traversed', 4, {}, { nodeKey: 'x', edgeKey: 'xa' }, 'choice'),
    ]);
    const invocation = projection.invocations[0];
    expect([...invocation.traversed]).toEqual(['xa']);
    expect([...invocation.skippedNodes]).toEqual(['b']);
    expect([...invocation.skippedEdges]).toEqual(['xb', 'bm']);
    expect(nodeProgress(invocation, 'm')).toBe('pending');
    expect(invocation.nodes[0].route).toBe('Selected route: Healthy route');
    expect(invocation.nodes[0].events.some((event) => event.type === 'edge.traversed')).toBe(false);
  });
  it('leaves missing history unknown without inventing start times or successful completion', () => {
    const projection = projectRun([begin(), response(5), event('step.finished', 6)]);
    expect(projection.incomplete).toBe(true);
    expect(duration(projection.invocations[0].nodes[0])).toBe('—');
    const unfinished = projectRun(
      [begin(), event('step.started', 2, { kind: 'http' })],
      completeReport([begin(), event('step.started', 2, { kind: 'http' })])
    );
    expect(unfinished.invocations[0].nodes[0].state).toBe('unknown');
    expect(nodeProgress(unfinished.invocations[0], 'e')).toBe('unknown');
    const crashed = projectRun([begin(), event('step.started', 2, { kind: 'http' })], null, 'error');
    expect(crashed.invocations[0].nodes[0].state).toBe('unknown');
    expect(crashed.incomplete).toBe(true);
    expect(projectRun([], null, 'error').incomplete).toBe(false);
  });
  it('distinguishes cancellation and failure and keeps lifecycle scopes separate', () => {
    const failed = projectRun([
      begin(),
      event('step.started', 2, { kind: 'http' }),
      event('step.failed', 3, { error: { kind: 'timeout', message: 'Request timed out' } }),
    ]);
    expect(failed.invocations[0].nodes[0]).toMatchObject({ state: 'failed', error: 'timeout: Request timed out' });
    const cancelled = projectRun([
      begin(),
      event('step.started', 2, { kind: 'http' }),
      event('run.cancelled', 3, {}, { invocationId: null, nodeKey: null }, null),
    ]);
    expect(cancelled.invocations[0].nodes[0].state).toBe('cancelled');
    const phases = projectRun([
      begin(),
      event('step.started', 2, { kind: 'http' }, { phase: 'initialize' }),
      event('step.finished', 3, {}, { phase: 'initialize' }),
      event('step.started', 4, { kind: 'http' }, { phase: 'cleanup' }),
    ]);
    expect(phases.invocations.map((invocation) => invocation.phase)).toEqual(['initialize', 'cleanup']);
  });
  it('handles batched, duplicate, out-of-order and wrong-run observations deterministically', () => {
    const events = [begin(), event('step.started', 2, { kind: 'http' }), response(3), event('step.finished', 4)];
    const projection = projectRun([
      events[3],
      events[1],
      events[2],
      events[0],
      events[2],
      { ...response(3), runId: 'other' },
    ]);
    expect(projection.invocations[0].nodes[0].events).toHaveLength(3);
    expect(projection.invocations[0].nodes[0].state).toBe('completed');
    expect(projection.incomplete).toBe(false);
  });
  it('reconciles recovered live delivery while preserving truncation limitations', () => {
    const events = [begin(), event('step.started', 2, { kind: 'http' }), response(3), event('step.finished', 4)];
    const report = { ...completeReport(events), deliveryDroppedEvents: 2 };
    expect(projectRun(report.events, report).incomplete).toBe(false);
    expect(projectRun(report.events, { ...report, eventsTruncated: true }).incomplete).toBe(true);
  });
});
