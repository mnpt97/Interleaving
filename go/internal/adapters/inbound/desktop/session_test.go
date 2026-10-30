package desktop_test

import (
	"context"
	"encoding/json"
	"errors"
	"interleaving/internal/adapters/inbound/desktop"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/application"
	"interleaving/internal/bootstrap"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func scenario(t *testing.T, base string) string {
	t.Helper()
	source, err := os.ReadFile("../../../../../scenarios/examples/hello-api.interl.json")
	if err != nil {
		t.Fatal(err)
	}
	return strings.ReplaceAll(string(source), "http://127.0.0.1:8080", base)
}
func TestStreamProtocolRetainsReportBeforeTerminalDelivery(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(`{"ok":true}`)) }))
	defer server.Close()
	s := desktop.New(bootstrap.Workspace(charmlog.New(io.Discard)))
	var messages []desktop.Message
	s.Serve(context.Background(), scenario(t, server.URL), "json", func(m desktop.Message) error {
		messages = append(messages, m)
		if m.Kind == "finished" {
			result := s.SaveReport(context.Background(), filepath.Join(t.TempDir(), "report.json"), m.Report.RunID)
			if len(result.Diagnostics) != 0 {
				t.Error("report not committed before callback")
			}
		}
		return nil
	})
	if messages[0].Kind != "started" || messages[len(messages)-1].Kind != "finished" {
		t.Fatal("missing handshake or terminal report")
	}
	terminal := messages[len(messages)-1].Report
	if terminal.Status != "passed" || terminal.Summary.Requests != 1 || len(messages) != len(terminal.Events)+2 {
		t.Fatal("stream/report mismatch")
	}
	for i, m := range messages[1 : len(messages)-1] {
		if m.ProtocolVersion != 3 || m.Event == nil || m.Event.RunID != terminal.RunID || m.Event.Sequence != uint64(i+1) {
			t.Fatal("unsafe ordering or identity")
		}
	}
	if result := s.SaveReport(context.Background(), filepath.Join(t.TempDir(), "stale.json"), "stale"); len(result.Diagnostics) == 0 {
		t.Fatal("stale report exported")
	}
	if s.Cancel(terminal.RunID) {
		t.Fatal("completed run still active")
	}
	s.Serve(context.Background(), "{", "json", func(desktop.Message) error { return nil })
	if result := s.SaveReport(context.Background(), filepath.Join(t.TempDir(), "after-rejection.json"), terminal.RunID); len(result.Diagnostics) != 0 {
		t.Fatal("rejected start erased previous report")
	}
}

func TestDisconnectedAcknowledgementSendsNoHTTPAndNextRunCanStart(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { calls.Add(1); _, _ = w.Write([]byte(`null`)) }))
	defer server.Close()
	s := desktop.New(bootstrap.Workspace(charmlog.New(io.Discard)))
	s.Serve(context.Background(), scenario(t, server.URL), "json", func(desktop.Message) error { return errors.New("private transport error") })
	if calls.Load() != 0 {
		t.Fatal("HTTP before acknowledgement delivery")
	}
	finished := false
	s.Serve(context.Background(), scenario(t, server.URL), "json", func(m desktop.Message) error { finished = finished || m.Kind == "finished"; return nil })
	if !finished || calls.Load() != 1 {
		t.Fatal("failed start left run locked")
	}
}

func TestOnlyOneActiveRunAndCancellationPreservesPartialReport(t *testing.T) {
	entered := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { close(entered); <-r.Context().Done() }))
	defer server.Close()
	s := desktop.New(bootstrap.Workspace(charmlog.New(io.Discard)))
	var mu sync.Mutex
	id := ""
	var reportStatus string
	done := make(chan struct{})
	go func() {
		defer close(done)
		s.Serve(context.Background(), scenario(t, server.URL), "json", func(m desktop.Message) error {
			mu.Lock()
			defer mu.Unlock()
			if m.Kind == "started" {
				id = m.RunID
			}
			if m.Kind == "finished" {
				reportStatus = m.Report.Status
			}
			return nil
		})
	}()
	select {
	case <-entered:
	case <-time.After(3 * time.Second):
		t.Fatal("HTTP did not start")
	}
	var rejected desktop.Message
	s.Serve(context.Background(), scenario(t, server.URL), "json", func(m desktop.Message) error { rejected = m; return nil })
	if rejected.Kind != "rejected" {
		t.Fatal("concurrent run was not rejected")
	}
	if s.Cancel("foreign") {
		t.Fatal("foreign id cancelled active run")
	}
	mu.Lock()
	runID := id
	mu.Unlock()
	if !s.Cancel(runID) {
		t.Fatal("cancellation failed")
	}
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("cancel did not finish")
	}
	if reportStatus != "cancelled" {
		t.Fatal("partial report not delivered")
	}
}

func TestMalformedStartIsSafelyRejected(t *testing.T) {
	s := desktop.New(bootstrap.Workspace(charmlog.New(io.Discard)))
	var rejected desktop.Message
	s.Serve(context.Background(), "{", "json", func(m desktop.Message) error { rejected = m; return nil })
	wire, _ := json.Marshal(rejected)
	if rejected.Kind != "rejected" || len(rejected.Diagnostics) == 0 || strings.Contains(string(wire), "private") {
		t.Fatal("unsafe rejection")
	}
}

// Delay admission to reproduce shutdown while a provider/codec is still preparing.
type preparingCore struct {
	application.WorkspacePort
	entered chan struct{}
}

func (c preparingCore) Start(ctx context.Context, _ application.RunRequest) (*application.RunHandle, error) {
	close(c.entered)
	<-ctx.Done()
	return nil, ctx.Err()
}
func TestClosingOwnerInterruptsPreparationAndRejectsFutureStarts(t *testing.T) {
	core := preparingCore{WorkspacePort: bootstrap.Workspace(charmlog.New(io.Discard)), entered: make(chan struct{})}
	s := desktop.New(core)
	done := make(chan struct{})
	go func() {
		defer close(done)
		s.Serve(context.Background(), "{}", "json", func(desktop.Message) error { return nil })
	}()
	<-core.entered
	s.Close()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("preparation leaked after close")
	}
	var rejected desktop.Message
	s.Serve(context.Background(), "{}", "json", func(m desktop.Message) error { rejected = m; return nil })
	if rejected.Kind != "rejected" {
		t.Fatal("closed owner admitted a new run")
	}
}

func TestStreamDisconnectInterruptsInFlightHTTPAndRetainsReport(t *testing.T) {
	entered := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { close(entered); <-r.Context().Done() }))
	defer server.Close()
	s := desktop.New(bootstrap.Workspace(charmlog.New(io.Discard)))
	ctx, disconnect := context.WithCancel(context.Background())
	defer disconnect()
	id := make(chan string, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		s.Serve(ctx, scenario(t, server.URL), "json", func(m desktop.Message) error {
			if m.Kind == "started" {
				id <- m.RunID
			}
			return nil
		})
	}()
	<-entered
	disconnect()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("disconnected receiver kept HTTP alive")
	}
	result := s.SaveReport(context.Background(), filepath.Join(t.TempDir(), "partial.json"), <-id)
	if len(result.Diagnostics) != 0 {
		t.Fatal("partial report was not retained after disconnect")
	}
}
