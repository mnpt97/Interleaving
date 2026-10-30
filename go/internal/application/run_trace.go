package application

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"interleaving/internal/domain"
	"time"
)

type retainedResponse struct {
	response Response
	parsed   any
	decoded  bool
}
type frame struct {
	flow, current  int
	invocation     string
	parent, caller *string
	values         map[string]any
	responses      map[string]*retainedResponse
	pending        bool
}
type runner struct {
	workspace           *Workspace
	program             *domain.Program
	session             Session
	security            *security
	credentials         map[string]string
	options             RunOptions
	handle              *RunHandle
	events              chan RunEvent
	report              RunReport
	reportBytes         int
	start               time.Time
	phase               string
	activeContext       context.Context
	frames              []*frame
	visits, invocations int
}

func integerText(n int) string { return fmt.Sprintf("%d", n) }
func (r *runner) elapsed() uint64 {
	elapsed := r.workspace.clock.Now().Sub(r.start)
	if elapsed < 0 {
		return 0
	}
	return uint64(elapsed)
}
func (r *runner) context(f *frame, node, edge int) EventContext {
	c := EventContext{GroupKey: pointer("group:0")}
	if r.phase != "" {
		c.Phase = pointer(r.phase)
	}
	if r.phase == "iteration" {
		v := uint32(1)
		c.Iteration = &v
	}
	if f != nil {
		c.WorkflowKey = pointer("wf:" + integerText(f.flow))
		c.InvocationID = pointer(f.invocation)
		c.ParentInvocationID = f.parent
		c.CallerNodeKey = f.caller
		if node >= 0 {
			c.NodeKey = pointer(nodeKey(f.flow, node))
		}
		if edge >= 0 {
			c.EdgeKey = pointer(edgeKey(f.flow, edge))
		}
	}
	return c
}
func (r *runner) boundary() *Failure {
	if r.activeContext != nil && r.activeContext.Err() != nil {
		if r.phase == "cleanup" && r.handle.control.owner.Err() == nil {
			return fail("cleanupTimeout", "Cleanup deadline exceeded")
		}
		return fail("cancelled", "Run cancelled")
	}
	if r.handle.control.owner.Err() != nil {
		return fail("cancelled", "Run owner stopped")
	}
	return nil
}
func (r *runner) emit(f *frame, node, edge int, kind string, payload any, terminal bool) *Failure {
	if !terminal {
		if problem := r.boundary(); problem != nil {
			return problem
		}
	}
	c := r.context(f, node, edge)
	if kind == "run.started" || kind == "run.finished" || kind == "run.cancelled" {
		c = EventContext{}
	}
	var name *string
	if f != nil && node >= 0 {
		name = pointer(label(r.security, r.program.Document.Workflows[f.flow].Nodes[node].ID))
	}
	event := RunEvent{RunID: r.handle.ID, Sequence: uint64(len(r.report.Events) + 1), ElapsedNS: r.elapsed(), UserID: pointer("vu:1"), NodeID: name, Type: kind, Payload: payload, Context: c}
	wire, err := json.Marshal(event)
	if err != nil {
		return fail("resourceLimit", "Event serialization failed")
	}
	if !terminal && (r.reportBytes+len(wire)+1 > r.options.MaxReportBytes-65536 || len(r.report.Events) >= 1984) {
		r.report.EventsTruncated = true
		r.handle.control.stop(SafetyStop)
		return fail("resourceLimit", "Report retention limit reached")
	}
	r.reportBytes += len(wire) + 1
	r.report.Events = append(r.report.Events, event)
	// Isolate mutable payload maps from public consumers and retained report data.
	var public RunEvent
	decoder := json.NewDecoder(bytes.NewReader(wire))
	decoder.UseNumber()
	_ = decoder.Decode(&public)
	if r.handle.control.owner.Err() != nil || (r.phase != "cleanup" && r.handle.control.work.Err() != nil) {
		select {
		case r.events <- public:
		default:
			r.report.DeliveryDroppedEvents++
		}
		return nil
	}
	ctx := r.activeContext
	if ctx == nil {
		ctx = r.handle.control.work
	}
	if terminal && r.phase == "cleanup" {
		ctx = r.handle.control.owner
	}
	select {
	case r.events <- public:
		return nil
	case <-ctx.Done():
		r.report.DeliveryDroppedEvents++
		return r.boundary()
	}
}
func (r *runner) run() {
	empty := map[string]any{}
	r.activeContext = r.handle.control.work
	primary := r.emit(nil, -1, -1, "run.started", map[string]any{"scenarioId": r.security.text(r.program.Document.ID), "graph": r.report.Graph}, false)
	if primary == nil {
		primary = r.emit(nil, -1, -1, "user.spawned", empty, false)
	}
	l := r.program.Document.Groups[0].Lifecycle
	init := map[string]any{}
	if primary == nil && l.Initialize != nil {
		r.phase = "initialize"
		init, primary = r.root(*l.Initialize, nil)
	}
	if primary == nil {
		r.phase = "iteration"
		_, primary = r.root(l.Iteration, init)
	}
	if l.Cleanup != nil {
		r.phase = "cleanup"
		reason := ""
		if r.report.EventsTruncated || safetyFailure(primary) {
			reason = "safetyLimit"
		} else {
			switch StopCause(r.handle.control.cause.Load()) {
			case OwnerClosed:
				reason = "ownerClosed"
			case ReceiverClosed:
				reason = "receiverClosed"
			case SafetyStop:
				reason = "safetyLimit"
			}
		}
		if reason != "" {
			r.report.Cleanup = CleanupOutcome{Status: "skipped", Reason: pointer(reason)}
			_ = r.emit(nil, -1, -1, "cleanup.skipped", map[string]any{"reason": reason}, true)
		} else {
			cleanup, cancel := r.workspace.runs.Timers.Timeout(r.handle.control.owner, time.Duration(l.CleanupTimeoutMS)*time.Millisecond)
			r.activeContext = cleanup
			_, problem := r.root(*l.Cleanup, nil)
			cancel()
			r.report.Cleanup.Status = "completed"
			if problem != nil {
				r.report.Cleanup.Status = "error"
				if problem.Kind == "cancelled" {
					r.report.Cleanup.Status = "cancelled"
				}
				r.report.Cleanup.Error = problem
				if primary == nil {
					primary = problem
				}
			}
		}
	}
	if primary == nil && r.handle.control.work.Err() != nil {
		primary = fail("cancelled", "Run cancelled")
	}
	r.phase = ""
	r.activeContext = r.handle.control.owner
	r.report.Status = status(primary)
	r.report.Error = primary
	_ = r.emit(nil, -1, -1, "user.finished", map[string]any{"status": r.report.Status}, true)
	if r.report.Status == "cancelled" {
		_ = r.emit(nil, -1, -1, "run.cancelled", empty, true)
	}
	_ = r.emit(nil, -1, -1, "run.finished", map[string]any{"status": r.report.Status}, true)
	r.report.Summary.ElapsedNS = r.elapsed()
}
func status(problem *Failure) string {
	if problem == nil {
		return "passed"
	}
	if problem.Kind == "cancelled" {
		return "cancelled"
	}
	return "error"
}
func safetyFailure(problem *Failure) bool {
	if problem == nil {
		return false
	}
	switch problem.Kind {
	case "resourceLimit", "valueLimit", "invocationLimit", "templateLimit", "extractionOutputLimit", "extractionResponseLimit", "extractionValueLimit", "extractionJsonDepthLimit", "extractionJsonValueLimit":
		return true
	}
	return false
}
