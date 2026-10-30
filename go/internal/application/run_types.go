package application

import (
	"bytes"
	"context"
	"encoding/json"
	"sync/atomic"
	"time"

	"interleaving/internal/domain"
)

type Failure struct {
	Kind    string `json:"kind"`
	Message string `json:"message"`
}

func (f *Failure) Error() string { return f.Kind + ": " + f.Message }
func fail(kind, message string) *Failure {
	if safe, ok := extractionMessages[kind]; ok {
		message = safe
	}
	if safe, ok := runtimeMessages[kind]; ok {
		message = safe
	}
	return &Failure{Kind: kind, Message: message}
}

type RunOptions struct {
	MaxResponseBytes, MaxRequestBytes, MaxPreviewBytes, MaxReportBytes, EventCapacity int
	SensitiveFields                                                                   []string
}

func DefaultRunOptions() RunOptions { return RunOptions{65536, 65536, 16384, 8388608, 64, nil} }

type RunRequest struct {
	Source  []byte
	Format  string
	Options RunOptions
}

// Private HTTP data crosses only the transport/application seam, never the run
// event/report ports. Each Session belongs to one run/VU and is closed by it.
type HTTPInput struct {
	BaseURL, Method, Path string
	Headers               map[string]string
	Body                  []byte
	ContentType           string
	JSONBody              bool
	TimeoutMS             uint64
}
type Request struct {
	URL, Method string
	Headers     map[string]string
	Body        []byte
	TimeoutMS   uint64
	JSONBody    bool
}
type Response struct {
	Status    uint16
	Headers   map[string][]string
	Body      []byte
	Truncated bool
}
type Session interface {
	Prepare(HTTPInput) (Request, *Failure)
	Do(context.Context, Request) (Response, *Failure)
	UpdateHeaders(map[string]string, []string) *Failure
	Reset() *Failure
	HasResponseCookies(string, Response) bool
	Close()
}
type Sessions interface {
	New(domain.ClientProfile, RunOptions) (Session, *Failure)
}
type Credentials interface {
	Lookup(context.Context, string) (string, error)
}
type ResponseJSON interface {
	Parse(context.Context, []byte) (any, *Failure)
}
type URLValues interface {
	Encode(string) string
	HideQuery(string, string) (string, []string)
}

var ErrMissingCredential = &Failure{Kind: "missingSecret", Message: "A required environment credential is missing."}

// Timers owns deadline creation; deterministic tests can replace actual timers.
type Timers interface {
	Timeout(context.Context, time.Duration) (context.Context, context.CancelFunc)
}
type ReportFiles interface {
	Check(context.Context, string, string) error
	Write(context.Context, string, string, []byte) error
}
type RunServices struct {
	Sessions    Sessions
	Credentials Credentials
	Timers      Timers
	Reports     ReportFiles
	JSON        ResponseJSON
	URLs        URLValues
}

func WithRuns(services RunServices) func(*Workspace) {
	return func(w *Workspace) { w.runs = &services }
}

type EventContext struct {
	GroupKey           *string `json:"groupKey"`
	Phase              *string `json:"phase"`
	Iteration          *uint32 `json:"iteration"`
	WorkflowKey        *string `json:"workflowKey"`
	InvocationID       *string `json:"invocationId"`
	ParentInvocationID *string `json:"parentInvocationId"`
	CallerNodeKey      *string `json:"callerNodeKey"`
	NodeKey            *string `json:"nodeKey"`
	EdgeKey            *string `json:"edgeKey"`
}
type RunEvent struct {
	RunID     string       `json:"runId"`
	Sequence  uint64       `json:"sequence"`
	ElapsedNS uint64       `json:"elapsedNs"`
	UserID    *string      `json:"userId"`
	NodeID    *string      `json:"nodeId"`
	Type      string       `json:"type"`
	Payload   any          `json:"payload"`
	Context   EventContext `json:"context"`
}
type CleanupOutcome struct {
	Status string   `json:"status"`
	Error  *Failure `json:"error"`
	Reason *string  `json:"reason,omitempty"`
}
type RunSummary struct {
	Requests           uint32 `json:"requests"`
	Responses          uint32 `json:"responses"`
	HTTPErrorResponses uint32 `json:"httpErrorResponses"`
	ElapsedNS          uint64 `json:"elapsedNs"`
}
type RunReport struct {
	ReportVersion         uint32         `json:"reportVersion"`
	RunID                 string         `json:"runId"`
	Status                string         `json:"status"`
	Snapshot              any            `json:"snapshot"`
	Graph                 any            `json:"graph"`
	Cleanup               CleanupOutcome `json:"cleanup"`
	Summary               RunSummary     `json:"summary"`
	Events                []RunEvent     `json:"events"`
	Error                 *Failure       `json:"error"`
	EventsTruncated       bool           `json:"eventsTruncated"`
	DeliveryDroppedEvents uint64         `json:"deliveryDroppedEvents"`
}
type StopCause uint32

const (
	UserCancel StopCause = iota + 1
	OwnerClosed
	ReceiverClosed
	SafetyStop
)

type runControl struct {
	cause            atomic.Uint32
	cancel, shutdown context.CancelFunc
	work, owner      context.Context
}

func newControl(parent context.Context) *runControl {
	owner, shutdown := context.WithCancel(context.WithoutCancel(parent))
	work, cancel := context.WithCancel(owner)
	return &runControl{cancel: cancel, shutdown: shutdown, work: work, owner: owner}
}
func (c *runControl) stop(cause StopCause) {
	for {
		old := c.cause.Load()
		if old >= uint32(cause) || c.cause.CompareAndSwap(old, uint32(cause)) {
			break
		}
	}
	c.cancel()
	if cause > UserCancel {
		c.shutdown()
	}
}

// RunHandle owns bounded delivery. Consumers drain Events until closed; Close
// explicitly disposes the owner and stops cleanup. CloseReceiver records a
// detached event consumer. Consumers must drain or close; Wait is repeatable.
type RunHandle struct {
	ID      string
	Events  <-chan RunEvent
	control *runControl
	done    chan struct{}
	report  RunReport
}

func (h *RunHandle) Cancel()        { h.control.stop(UserCancel) }
func (h *RunHandle) Close()         { h.control.stop(OwnerClosed) }
func (h *RunHandle) CloseReceiver() { h.control.stop(ReceiverClosed) }
func (h *RunHandle) Wait(ctx context.Context) (RunReport, error) {
	select {
	case <-ctx.Done():
		return RunReport{}, ctx.Err()
	case <-h.done:
		data, err := json.Marshal(h.report)
		if err != nil {
			return RunReport{}, fail("reportRead", "Report serialization failed")
		}
		var owned RunReport
		decoder := json.NewDecoder(bytes.NewReader(data))
		decoder.UseNumber()
		if err = decoder.Decode(&owned); err != nil {
			return RunReport{}, fail("reportRead", "Report decoding failed")
		}
		return owned, nil
	}
}
