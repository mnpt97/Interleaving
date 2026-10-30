// Package application owns use cases and their consumer-defined dependency ports.
package application

import (
	"context"
	"errors"
	"sync"
	"time"

	"interleaving/internal/domain"
)

// Clock is a replaceable source of time; application code never reads ambient time.
type Clock interface{ Now() time.Time }

// IDs provides opaque identities without coupling the core to a UUID library.
type IDs interface{ NewID() string }

// Scenarios is the driving port for the first scenario migration use case.
type Scenarios interface {
	Validate(context.Context, string, string) error
	ValidateFile(context.Context, string) error
	Inspect(context.Context, string, string) ValidationResult
	OpenScenario(context.Context, string) (OpenResult, error)
	SaveDraft(context.Context, string, string) (ValidationResult, error)
}

// Runs is independent of the CLI, desktop runtime and event serialization.
type Runs interface {
	Start(context.Context, RunRequest) (*RunHandle, error)
	StartFile(context.Context, string, RunOptions) (*RunHandle, error)
	Cancel(string) bool
	CheckReportPath(context.Context, string, string) error
	SaveReport(context.Context, string, string, RunReport) error
}

// WorkspacePort is the driving contract consumed by CLI and desktop adapters.
// A replacement application implementation need not share Workspace's concrete type.
type WorkspacePort interface {
	Scenarios
	Runs
	Capabilities() domain.Capabilities
	Probe(context.Context, int) (<-chan ProbeEvent, error)
}

// ProbeEvent is a synthetic bridge message, deliberately separate from execution protocol v3.
type ProbeEvent struct {
	ID        string    `json:"id"`
	Sequence  int       `json:"sequence"`
	At        time.Time `json:"at"`
	Synthetic bool      `json:"synthetic"`
}

// Workspace exposes bounded shell capabilities before scenario execution is ported.
type Workspace struct {
	clock     Clock
	ids       IDs
	logger    Logger
	scenarios *ScenarioServices
	runs      *RunServices
	runMu     sync.Mutex
	active    map[string]*RunHandle
}

func NewWorkspace(clock Clock, ids IDs, logger Logger, options ...func(*Workspace)) *Workspace {
	w := &Workspace{clock: clock, ids: ids, logger: logger, active: map[string]*RunHandle{}}
	for _, option := range options {
		option(w)
	}
	return w
}
func (w *Workspace) Capabilities() domain.Capabilities {
	return domain.Capabilities{BridgeProbe: true, Validation: w.scenarios != nil, Execution: w.runs != nil}
}

// Probe fills a bounded stream and closes it before returning, so abandoned consumers cannot leak workers.
func (w *Workspace) Probe(ctx context.Context, count int) (<-chan ProbeEvent, error) {
	if count < 1 || count > 16 {
		return nil, errors.New("probe count must be between 1 and 16")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	events := make(chan ProbeEvent, count)
	id := w.ids.NewID()
	for i := 1; i <= count; i++ {
		if err := ctx.Err(); err != nil {
			close(events)
			return nil, err
		}
		events <- ProbeEvent{ID: id, Sequence: i, At: w.clock.Now(), Synthetic: true}
	}
	close(events)
	w.logger.Log(ctx, Info, "bridge probe completed")
	return events, nil
}

var _ WorkspacePort = (*Workspace)(nil)
