import type { RunEvent } from '../../lib/run-types';
import type { NodeExecution } from './projection';
import { duration } from './projection';

export function EventDetails({ event }: { event?: RunEvent }) {
  if (!event) return <p className="muted">Select an event.</p>;
  return (
    <>
      <h3>
        {event.type}
        {event.type === 'response.received' ? ` · HTTP ${event.payload.status}` : ''}
      </h3>
      {event.type === 'session.updated' && (
        <section aria-label="Session changes">
          <dl className="details">
            <div>
              <dt>Set</dt>
              <dd>{event.payload.setHeaders.join(', ') || 'None'}</dd>
            </div>
            <div>
              <dt>Removed</dt>
              <dd>{event.payload.removedHeaders.join(', ') || 'None'}</dd>
            </div>
          </dl>
        </section>
      )}
      {(event.type === 'request.sent' || event.type === 'response.received') && event.payload.body.omitted && (
        <p className="payload-omission">
          {event.payload.body.reason === 'Sensitive authentication payload omitted'
            ? 'Authentication payload hidden.'
            : event.payload.body.reason || 'Payload unavailable.'}
        </p>
      )}
      <pre>{JSON.stringify(event.payload, null, 2)}</pre>
    </>
  );
}
export function ExecutionDetails({ node }: { node: NodeExecution }) {
  return (
    <dl className="details">
      <div>
        <dt>State</dt>
        <dd>{node.state}</dd>
      </div>
      <div>
        <dt>Duration</dt>
        <dd>{duration(node)}</dd>
      </div>
      {node.httpStatus !== undefined && (
        <div>
          <dt>HTTP status</dt>
          <dd>{node.httpStatus}</dd>
        </div>
      )}
      {node.route && (
        <div>
          <dt>Route</dt>
          <dd>{node.route}</dd>
        </div>
      )}
    </dl>
  );
}
