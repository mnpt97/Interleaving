package application_test

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"interleaving/internal/adapters/outbound/httpclient"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

// Function-backed transport exercises the real preparation/header adapter while
// replacing only HTTP I/O. Bounds remain those of the public transport port.
type responseFactory struct {
	respond func(application.Request) application.Response
}
type responseSession struct {
	application.Session
	factory responseFactory
	max     int
}

func (f responseFactory) New(p domain.ClientProfile, o application.RunOptions) (application.Session, *application.Failure) {
	s, err := (httpclient.Adapter{}).New(p, o)
	return &responseSession{Session: s, factory: f, max: o.MaxResponseBytes}, err
}
func (s *responseSession) Do(_ context.Context, r application.Request) (application.Response, *application.Failure) {
	response := s.factory.respond(r)
	if len(response.Body) > s.max {
		response.Body = response.Body[:s.max]
		response.Truncated = true
	}
	return response, nil
}
func sourceMap(t *testing.T, source []byte) map[string]any {
	t.Helper()
	var doc map[string]any
	if err := json.Unmarshal(source, &doc); err != nil {
		t.Fatal(err)
	}
	return doc
}
func encoded(t *testing.T, doc map[string]any) []byte {
	t.Helper()
	wire, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	return wire
}
func line(id string, nodes []any) map[string]any {
	edges := []any{}
	for i := 1; i < len(nodes); i++ {
		edges = append(edges, map[string]any{"id": fmt.Sprintf("e%d", i), "from": nodes[i-1].(map[string]any)["id"], "to": nodes[i].(map[string]any)["id"]})
	}
	return map[string]any{"id": id, "name": id, "nodes": nodes, "edges": edges}
}
func node(id, kind string, config map[string]any) map[string]any {
	n := map[string]any{"id": id, "type": kind}
	if config != nil {
		n["config"] = config
	}
	return n
}
func basicDoc(t *testing.T) map[string]any {
	doc := sourceMap(t, lifecycleSource(t, "http://fixture.invalid", 100))
	delete(doc["groups"].([]any)[0].(map[string]any)["lifecycle"].(map[string]any), "cleanup")
	return doc
}
func runMap(t *testing.T, doc map[string]any, options application.RunOptions, response func(application.Request) application.Response) application.RunReport {
	t.Helper()
	app, _ := runWorkspace(t, responseFactory{respond: response}, testCredentials{})
	h, err := app.Start(context.Background(), application.RunRequest{Source: encoded(t, doc), Format: "json", Options: options})
	if err != nil {
		t.Fatal(err)
	}
	return collectRun(t, h)
}

func TestAggregateInputsCountSuspendedFrames(t *testing.T) {
	doc := basicDoc(t)
	parentInputs, childInputs, mappings, fields := map[string]any{}, map[string]any{}, map[string]any{}, map[string]any{}
	for i := 0; i < 7; i++ {
		name := fmt.Sprintf("v%d", i)
		parentInputs[name] = map[string]any{"type": "string", "default": strings.Repeat("x", 8192)}
		childInputs[name] = map[string]any{"type": "string"}
		mappings[name] = map[string]any{"source": "input", "name": name}
		fields[name] = mappings[name]
	}
	parent := line("main", []any{node("start", "start", nil), node("child", "call", map[string]any{"workflowId": "child", "inputs": mappings}), node("end", "end", nil)})
	parent["inputs"] = parentInputs
	child := line("child", []any{node("start", "start", nil), node("http", "http", map[string]any{"method": "POST", "path": "/echo", "body": map[string]any{"kind": "jsonTemplate", "fields": fields}}), node("end", "end", nil)})
	child["inputs"] = childInputs
	doc["workflows"] = []any{parent, child}
	report := runMap(t, doc, application.DefaultRunOptions(), func(application.Request) application.Response {
		t.Error("HTTP ran past scalar budget")
		return application.Response{}
	})
	if report.Error == nil || report.Error.Kind != "valueLimit" || report.Summary.Requests != 0 {
		t.Fatalf("%+v %+v", report.Error, report.Summary)
	}
}

func TestRetainedResponsesShareBudgetAcrossFrames(t *testing.T) {
	doc := basicDoc(t)
	flows := []any{}
	for i := 0; i < 8; i++ {
		nodes := []any{node("start", "start", nil), node("body", "http", map[string]any{"method": "GET", "path": "/large"})}
		if i < 7 {
			nodes = append(nodes, node("child", "call", map[string]any{"workflowId": fmt.Sprintf("w%d", i+1)}))
		}
		nodes = append(nodes, node("extract", "extract", map[string]any{"sourceNodeId": "body", "expression": "response.body.token", "output": "token"}), node("end", "end", nil))
		flows = append(flows, line(fmt.Sprintf("w%d", i), nodes))
	}
	doc["workflows"] = flows
	doc["groups"].([]any)[0].(map[string]any)["lifecycle"].(map[string]any)["iteration"] = map[string]any{"workflowId": "w0"}
	opts := application.DefaultRunOptions()
	opts.MaxResponseBytes = 200000
	report := runMap(t, doc, opts, func(application.Request) application.Response {
		return application.Response{Status: 200, Body: []byte(`{"token":"fake-token","padding":"` + strings.Repeat("x", 131072) + `"}`)}
	})
	if report.Error == nil || report.Error.Kind != "valueLimit" || report.Summary.Requests != 8 {
		t.Fatalf("%+v %+v", report.Error, report.Summary)
	}
}

func TestNamedExtractionIsAtomicAndRespectsRemainingScalarBudget(t *testing.T) {
	for _, budget := range []bool{false, true} {
		t.Run(map[bool]string{false: "wrong-type", true: "scalar-budget"}[budget], func(t *testing.T) {
			doc := basicDoc(t)
			outputs, inputs, fields := map[string]any{}, map[string]any{}, map[string]any{}
			if budget {
				for i := 0; i < 7; i++ {
					name := fmt.Sprintf("v%d", i)
					inputs[name] = map[string]any{"type": "string", "default": strings.Repeat("x", 8190)}
					fields[name] = map[string]any{"source": "input", "name": name}
				}
			}
			outputs["a"] = map[string]any{"expression": "response.body.token", "type": "string"}
			if !budget {
				outputs["b"] = map[string]any{"expression": "response.body.flag", "type": "integer"}
			}
			w := line("main", []any{node("start", "start", nil), node("http", "http", map[string]any{"method": "GET", "path": "/body"}), node("extract", "extract", map[string]any{"sourceNodeId": "http", "outputs": outputs}), node("set", "setSession", map[string]any{"setHeaders": map[string]any{"authorization": map[string]any{"source": "extraction", "nodeId": "extract", "name": "a"}}}), node("never", "http", map[string]any{"method": "POST", "path": "/echo", "body": map[string]any{"kind": "jsonTemplate", "fields": fields}}), node("end", "end", nil)})
			if budget {
				w["inputs"] = inputs
			}
			doc["workflows"] = []any{w}
			report := runMap(t, doc, application.DefaultRunOptions(), func(application.Request) application.Response {
				if budget {
					return application.Response{Status: 200, Body: []byte(`{"token":"` + strings.Repeat("y", 8192) + `"}`)}
				}
				return application.Response{Status: 200, Body: []byte(`{"token":"private-value","flag":true}`)}
			})
			want := "extractionWrongType"
			if budget {
				want = "extractionOutputLimit"
			}
			if report.Error == nil || report.Error.Kind != want || report.Summary.Requests != 1 {
				t.Fatalf("%+v %+v", report.Error, report.Summary)
			}
			for _, event := range report.Events {
				if event.Type == "session.updated" {
					t.Fatal("partial extraction reached session")
				}
			}
		})
	}
}

func TestSuccessfulSmallEventBufferDeliversEveryTerminalEvent(t *testing.T) {
	doc := basicDoc(t)
	opts := application.DefaultRunOptions()
	opts.EventCapacity = 1
	report := runMap(t, doc, opts, func(application.Request) application.Response {
		return application.Response{Status: 200, Body: []byte(`null`)}
	})
	if report.Status != "passed" || report.DeliveryDroppedEvents != 0 {
		t.Fatalf("success lost terminal events: %d", report.DeliveryDroppedEvents)
	}
}

func TestReportBudgetStopsHTTPAndReservesTerminalFailure(t *testing.T) {
	doc := basicDoc(t)
	w := doc["workflows"].([]any)[0].(map[string]any)
	w["nodes"].([]any)[1].(map[string]any)["config"] = map[string]any{"method": "POST", "path": "/echo", "body": map[string]any{"kind": "json", "value": map[string]any{"data": strings.Repeat("x", 40000)}}}
	opts := application.DefaultRunOptions()
	opts.MaxReportBytes = 131072
	opts.MaxPreviewBytes = 65536
	report := runMap(t, doc, opts, func(r application.Request) application.Response {
		return application.Response{Status: 200, Body: r.Body}
	})
	wire, _ := json.Marshal(report)
	if report.Error == nil || report.Error.Kind != "resourceLimit" || !report.EventsTruncated || len(wire) > opts.MaxReportBytes || report.Events[len(report.Events)-1].Type != "run.finished" {
		t.Fatal("report budget did not preserve a bounded terminal failure")
	}
}

func TestConcurrentRunsNeverShareSessionOrInvocationValues(t *testing.T) {
	factory := responseFactory{respond: func(r application.Request) application.Response {
		return application.Response{Status: 200, Body: []byte(`{"token":"per-run"}`)}
	}}
	app, _ := runWorkspace(t, factory, testCredentials{})
	doc := basicDoc(t)
	source := encoded(t, doc)
	a, err := app.Start(context.Background(), application.RunRequest{Source: source, Format: "json"})
	if err != nil {
		t.Fatal(err)
	}
	b, err := app.Start(context.Background(), application.RunRequest{Source: source, Format: "json"})
	if err != nil {
		t.Fatal(err)
	}
	ra, rb := collectRun(t, a), collectRun(t, b)
	if a.ID == b.ID || ra.Status != "passed" || rb.Status != "passed" {
		t.Fatal("runs were not independent")
	}
	for _, e := range ra.Events {
		if e.RunID != a.ID {
			t.Fatal("foreign run event")
		}
	}
}

func TestStopDuringCleanupRespectsUserAndOwnerLifetimes(t *testing.T) {
	for _, owner := range []bool{false, true} {
		t.Run(map[bool]string{false: "user-cancel", true: "owner-disposed"}[owner], func(t *testing.T) {
			entered, release := make(chan struct{}), make(chan struct{})
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/cleanup" {
					close(entered)
					select {
					case <-release:
					case <-r.Context().Done():
						return
					}
				}
				_, _ = w.Write([]byte(`null`))
			}))
			defer server.Close()
			app, _ := runWorkspace(t, httpclient.Adapter{}, testCredentials{})
			h, err := app.Start(context.Background(), application.RunRequest{Source: lifecycleSource(t, server.URL, 1000), Format: "json"})
			if err != nil {
				t.Fatal(err)
			}
			stopped := make(chan struct{})
			workerContext, stopWorker := context.WithCancel(context.Background())
			defer stopWorker()
			go func() {
				defer close(stopped)
				select {
				case <-entered:
					if owner {
						h.Close()
					} else {
						h.Cancel()
					}
					close(release)
				case <-workerContext.Done():
				}
			}()
			report := collectRun(t, h)
			stopWorker()
			<-stopped
			want := "completed"
			if owner {
				want = "cancelled"
			}
			if report.Status != "cancelled" || report.Cleanup.Status != want {
				t.Fatalf("status=%s cleanup=%+v", report.Status, report.Cleanup)
			}
		})
	}
}

func TestDetachedReceiverPreservesTerminalReportAndSkipReason(t *testing.T) {
	app, _ := runWorkspace(t, responseFactory{respond: func(application.Request) application.Response {
		t.Error("HTTP after receiver disposal")
		return application.Response{}
	}}, testCredentials{})
	opts := application.DefaultRunOptions()
	opts.EventCapacity = 1
	h, err := app.Start(context.Background(), application.RunRequest{Source: lifecycleSource(t, "http://fixture.invalid", 1000), Format: "json", Options: opts})
	if err != nil {
		t.Fatal(err)
	}
	h.CloseReceiver()
	report, err := h.Wait(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if report.Status != "cancelled" || report.Cleanup.Status != "skipped" || report.Cleanup.Reason == nil || *report.Cleanup.Reason != "receiverClosed" || report.Events[len(report.Events)-1].Type != "run.finished" {
		t.Fatal("receiver shutdown lost terminal state")
	}
}
