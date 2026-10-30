// Package desktop adapts the shared application to a single desktop owner.
// It contains no native runtime, filesystem or logger implementation.
package desktop

import (
	"context"
	"interleaving/internal/application"
	"interleaving/internal/domain"
	"sync"
)

type Message struct {
	ProtocolVersion int                    `json:"protocolVersion"`
	Kind            string                 `json:"kind"`
	RunID           string                 `json:"runId,omitempty"`
	Event           *application.RunEvent  `json:"event,omitempty"`
	Report          *application.RunReport `json:"report,omitempty"`
	Diagnostics     domain.Diagnostics     `json:"diagnostics,omitempty"`
	Message         string                 `json:"message,omitempty"`
}
type Result struct {
	Value       any                `json:"value"`
	Diagnostics domain.Diagnostics `json:"diagnostics"`
}

func Success(value any) Result { return Result{Value: value, Diagnostics: domain.Diagnostics{}} }
func Failed(err error) Result  { return Result{Diagnostics: application.Diagnostics(err)} }

type pendingRun struct {
	handle *application.RunHandle
	cancel context.CancelFunc
}
type Session struct {
	core   application.WorkspacePort
	mu     sync.Mutex
	active *pendingRun
	report *application.RunReport
	closed bool
}

func New(core application.WorkspacePort) *Session { return &Session{core: core} }
func operation(message string) error {
	return domain.Problem("validation", "runOperation", "$", message)
}

// Serve owns one bounded stream. A started acknowledgement is queued before
// draining execution, so a failed acknowledgement cannot start HTTP.
func (s *Session) Serve(ctx context.Context, source, format string, send func(Message) error) {
	work, cancel := context.WithCancel(ctx)
	defer cancel()
	entry := &pendingRun{cancel: cancel}
	s.mu.Lock()
	if s.closed || s.active != nil {
		s.mu.Unlock()
		_ = send(Message{ProtocolVersion: 3, Kind: "rejected", Diagnostics: application.Diagnostics(operation("A run is already active or the window is closing."))})
		return
	}
	s.active = entry
	s.mu.Unlock()
	defer func() {
		s.mu.Lock()
		if s.active == entry {
			s.active = nil
		}
		s.mu.Unlock()
	}()
	options := application.DefaultRunOptions()
	options.EventCapacity = 1
	handle, err := s.core.Start(work, application.RunRequest{Source: []byte(source), Format: format, Options: options})
	if err != nil {
		_ = send(Message{ProtocolVersion: 3, Kind: "rejected", Diagnostics: application.Diagnostics(err)})
		return
	}
	defer handle.Close()
	s.mu.Lock()
	entry.handle = handle
	s.report = nil
	closed := s.closed
	s.mu.Unlock()
	if closed {
		handle.Close()
	}
	// Stream/page teardown is stronger than user cancellation and skips cleanup.
	detach := func() {
		s.mu.Lock()
		ownerClosed := s.closed
		s.mu.Unlock()
		if ownerClosed {
			handle.Close()
		} else {
			handle.CloseReceiver()
		}
	}
	stopDetach := context.AfterFunc(ctx, detach)
	defer stopDetach()
	alive := ctx.Err() == nil && !closed
	dropped := uint64(0)
	if !alive || send(Message{ProtocolVersion: 3, Kind: "started", RunID: handle.ID}) != nil {
		alive = false
		detach()
	}
	for event := range handle.Events {
		if alive {
			if err = send(Message{ProtocolVersion: 3, Kind: "event", Event: &event}); err != nil {
				alive = false
				detach()
				dropped++
			}
		} else {
			dropped++
		}
	}
	report, err := handle.Wait(context.Background())
	if err != nil {
		if alive {
			_ = send(Message{ProtocolVersion: 3, Kind: "failed", Message: "Execution did not produce a report."})
		}
		return
	}
	report.DeliveryDroppedEvents += dropped
	// Commit before terminal delivery: immediate export may race the stream callback.
	s.mu.Lock()
	if s.active == entry {
		s.report = &report
		s.active = nil
	}
	s.mu.Unlock()
	if alive {
		_ = send(Message{ProtocolVersion: 3, Kind: "finished", Report: &report})
	}
}
func (s *Session) Cancel(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.active == nil || s.active.handle == nil || s.active.handle.ID != id {
		return false
	}
	s.active.handle.Cancel()
	return true
}

// Close also interrupts preparation; no registry lock is held during admission.
func (s *Session) Close() {
	s.mu.Lock()
	s.closed = true
	entry := s.active
	if entry != nil {
		if entry.handle != nil {
			entry.handle.Close()
		}
		entry.cancel()
	}
	s.mu.Unlock()
}
func (s *Session) SaveReport(ctx context.Context, path, id string) Result {
	s.mu.Lock()
	report := s.report
	s.mu.Unlock()
	if report == nil || report.RunID != id {
		return Failed(operation("Completed report is no longer available."))
	}
	if err := s.core.SaveReport(ctx, "", path, *report); err != nil {
		return Failed(err)
	}
	return Success(true)
}
