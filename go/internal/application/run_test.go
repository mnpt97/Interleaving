package application_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/adapters/outbound/feel"
	"interleaving/internal/adapters/outbound/httpclient"
	"interleaving/internal/adapters/outbound/reportfiles"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/adapters/outbound/scenariofiles"
	"interleaving/internal/adapters/outbound/system"
	"interleaving/internal/adapters/outbound/targeturls"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

type testCredentials struct{ missing bool }

func (p testCredentials) Lookup(_ context.Context, name string) (string, error) {
	if p.missing {
		return "", application.ErrMissingCredential
	}
	if strings.Contains(name, "USERNAME") {
		return "sentinel-user", nil
	}
	return "sentinel-password", nil
}

type lockedBuffer struct {
	sync.Mutex
	bytes.Buffer
}

func (b *lockedBuffer) Write(p []byte) (int, error) {
	b.Lock()
	defer b.Unlock()
	return b.Buffer.Write(p)
}
func (b *lockedBuffer) text() string { b.Lock(); defer b.Unlock(); return b.Buffer.String() }

func runWorkspace(t *testing.T, sessions application.Sessions, credentials application.Credentials) (*application.Workspace, *lockedBuffer) {
	t.Helper()
	codec, err := scenariocodec.New()
	if err != nil {
		t.Fatal(err)
	}
	logs := &lockedBuffer{}
	var clock application.Clock = system.Clock{}
	if _, fake := sessions.(*scriptedSessions); fake {
		clock = fixedClock{}
	}
	return application.NewWorkspace(clock, system.IDs{}, charmlog.New(logs),
		application.WithScenarios(application.ScenarioServices{Codec: codec, Files: scenariofiles.Adapter{}, Expressions: feel.Adapter{}, Targets: targeturls.Adapter{}}),
		application.WithRuns(application.RunServices{Sessions: sessions, Credentials: credentials, Timers: system.Timers{}, Reports: reportfiles.Adapter{}, JSON: scenariocodec.ResponseJSON{}, URLs: targeturls.Adapter{}})), logs
}

func showcase(t *testing.T, format, base string, minimum int) []byte {
	t.Helper()
	source, err := os.ReadFile("../../../scenarios/examples/feature-showcase.interl." + format)
	if err != nil {
		t.Fatal(err)
	}
	source = bytes.ReplaceAll(source, []byte("http://127.0.0.1:8080"), []byte(base))
	if format == "yaml" {
		source = bytes.ReplaceAll(source, []byte("waitMs: 2000"), []byte("waitMs: 1"))
		if minimum == 4 {
			source = bytes.ReplaceAll(source, []byte("value: 1"), []byte("value: 4"))
		}
		return source
	}
	var doc map[string]any
	if err = json.Unmarshal(source, &doc); err != nil {
		t.Fatal(err)
	}
	doc["environment"].(map[string]any)["variables"].(map[string]any)["waitMs"] = 1
	doc["groups"].([]any)[0].(map[string]any)["lifecycle"].(map[string]any)["iteration"].(map[string]any)["inputs"].(map[string]any)["minimum"].(map[string]any)["value"] = minimum
	source, err = json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	return source
}

func collectRun(t *testing.T, h *application.RunHandle) application.RunReport {
	t.Helper()
	defer h.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	events := []application.RunEvent{}
	for {
		select {
		case event, ok := <-h.Events:
			if !ok {
				report, err := h.Wait(ctx)
				if err != nil {
					t.Fatal(err)
				}
				a, _ := json.Marshal(events)
				b, _ := json.Marshal(report.Events)
				if report.DeliveryDroppedEvents == 0 && !bytes.Equal(a, b) {
					t.Fatal("live events differ from retained report")
				}
				return report
			}
			events = append(events, event)
		case <-ctx.Done():
			t.Fatal("run did not terminate")
			return application.RunReport{}
		}
	}
}

// The same scripted journey is used by the fake port and real HTTP fixture.
// It asserts private request values; none may occur in the public report/logs.
func journey(t *testing.T, request application.Request) application.Response {
	t.Helper()
	u, err := url.Parse(request.URL)
	if err != nil {
		t.Fatal(err)
	}
	headers := map[string][]string{"Content-Type": {"application/json"}}
	body := `{"ok":true}`
	switch u.Path {
	case "/auth/login":
		var credentials map[string]string
		_ = json.Unmarshal(request.Body, &credentials)
		if credentials["username"] != "sentinel-user" || credentials["password"] != "sentinel-password" {
			t.Error("credentials not resolved through port")
		}
		if request.Headers["authorization"] != "" || request.Headers["cookie"] != "" {
			t.Error("authentication did not start a fresh session")
		}
		headers["Set-Cookie"] = []string{"sid=sentinel-cookie; Path=/; HttpOnly"}
		body = `{"accessToken":"sentinel-token"}`
	case "/auth/me":
		if request.Headers["authorization"] != "Bearer sentinel-token" {
			t.Error("bearer was not propagated")
		}
	case "/auth/cookie-me":
		if !strings.Contains(request.Headers["cookie"], "sid=sentinel-cookie") || request.Headers["authorization"] != "" {
			t.Error("cookie authentication/session reset failed")
		}
	case "/routing-body":
		body = `{"value":{"status":true,"attempts":3,"label":"ready"}}`
	case "/echo":
		if strings.Contains(string(request.Body), `"route":"accepted"`) {
			var body map[string]any
			_ = json.Unmarshal(request.Body, &body)
			if body["identity"] != "sentinel-user" || body["result"].(map[string]any)["ready"] != true {
				t.Error("typed call/template values missing")
			}
		}
	case "/long-poll":
		var body map[string]int
		_ = json.Unmarshal(request.Body, &body)
		if body["timeoutMs"] != 1 {
			t.Error("environment value not mapped to long poll")
		}
	case "/health":
		if request.Headers["x-showcase"] != "" {
			t.Error("cleanup did not remove session header")
		}
	default:
		t.Errorf("unexpected endpoint %s", u.Path)
	}
	return application.Response{Status: 200, Headers: headers, Body: []byte(body)}
}

type scriptedSessions struct {
	t      *testing.T
	closed bool
}
type scriptedSession struct {
	application.Session
	factory *scriptedSessions
	cookie  string
}

func (f *scriptedSessions) New(profile domain.ClientProfile, options application.RunOptions) (application.Session, *application.Failure) {
	s, err := (httpclient.Adapter{}).New(profile, options)
	return &scriptedSession{Session: s, factory: f}, err
}
func (s *scriptedSession) Do(_ context.Context, r application.Request) (application.Response, *application.Failure) {
	r.Headers["cookie"] = s.cookie
	response := journey(s.factory.t, r)
	if response.Headers["Set-Cookie"] != nil {
		s.cookie = "sid=sentinel-cookie"
	}
	return response, nil
}
func (s *scriptedSession) Reset() *application.Failure { s.cookie = ""; return s.Session.Reset() }
func (s *scriptedSession) HasResponseCookies(string, application.Response) bool {
	return s.cookie != ""
}
func (s *scriptedSession) Close() { s.Session.Close(); s.factory.closed = true }

func TestShowcaseVerticalSliceThroughFakeAndRealTransport(t *testing.T) {
	for _, real := range []bool{false, true} {
		for _, format := range []string{"json", "yaml"} {
			for _, minimum := range []int{1, 4} {
				name := format + "/accepted"
				if minimum == 4 {
					name = format + "/fallback"
				}
				if real {
					name += "/http"
				} else {
					name += "/fake"
				}
				t.Run(name, func(t *testing.T) {
					fake := &scriptedSessions{t: t}
					var sessions application.Sessions = fake
					base := "http://fixture.invalid"
					if real {
						server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
							data, _ := io.ReadAll(r.Body)
							headers := map[string]string{}
							for k, v := range r.Header {
								headers[strings.ToLower(k)] = strings.Join(v, ", ")
							}
							response := journey(t, application.Request{URL: r.URL.String(), Method: r.Method, Headers: headers, Body: data})
							for k, vs := range response.Headers {
								for _, v := range vs {
									w.Header().Add(k, v)
								}
							}
							w.WriteHeader(int(response.Status))
							_, _ = w.Write(response.Body)
						}))
						defer server.Close()
						base = server.URL
						sessions = httpclient.Adapter{}
					}
					app, logs := runWorkspace(t, sessions, testCredentials{})
					h, err := app.Start(context.Background(), application.RunRequest{Source: showcase(t, format, base, minimum), Format: format})
					if err != nil {
						t.Fatal(err)
					}
					report := collectRun(t, h)
					if report.Status != "passed" || report.Cleanup.Status != "completed" || report.Summary.Requests != 12 || report.Summary.Responses != 12 {
						t.Fatalf("unexpected outcome: %+v error=%+v cleanup=%+v", report.Summary, report.Error, report.Cleanup)
					}
					if !real && !fake.closed {
						t.Error("session resource not closed before Wait")
					}
					accepted, fallback, calls := false, false, 0
					for i, event := range report.Events {
						if event.Sequence != uint64(i+1) {
							t.Fatal("event ordering")
						}
						if event.Type == "step.started" && event.NodeID != nil {
							accepted = accepted || *event.NodeID == "accepted"
							fallback = fallback || *event.NodeID == "fallback"
						}
						if event.Type == "workflow.started" && event.Context.ParentInvocationID != nil {
							calls++
							if event.Context.CallerNodeKey == nil {
								t.Error("missing call scope")
							}
						}
					}
					if accepted != (minimum == 1) || fallback != (minimum == 4) || calls != 1 {
						t.Fatalf("routing/call mismatch %v %v %d", accepted, fallback, calls)
					}
					encoded, _ := json.Marshal(report)
					for _, secret := range []string{"sentinel-user", "sentinel-password", "sentinel-token", "sentinel-cookie"} {
						if bytes.Contains(encoded, []byte(secret)) || strings.Contains(logs.text(), secret) {
							t.Errorf("secret leaked: %s", secret)
						}
					}
					if !strings.Contains(logs.text(), "run started") || !strings.Contains(logs.text(), "run finished") {
						t.Error("Charm logging port not called")
					}
					// Returned reports and delivered events have independent ownership.
					report.Events[0].Payload = map[string]any{"changed": true}
					again, err := h.Wait(context.Background())
					if err != nil {
						t.Fatal(err)
					}
					if reflect.DeepEqual(report.Events[0].Payload, again.Events[0].Payload) {
						t.Error("report mutation reached retained state")
					}
				})
			}
		}
	}
}

func lifecycleSource(t *testing.T, base string, cleanupTimeout int) []byte {
	t.Helper()
	source, err := os.ReadFile("../../../scenarios/examples/hello-api.interl.json")
	if err != nil {
		t.Fatal(err)
	}
	var doc map[string]any
	_ = json.Unmarshal(bytes.ReplaceAll(source, []byte("http://127.0.0.1:8080"), []byte(base)), &doc)
	flows := doc["workflows"].([]any)
	copyBytes, _ := json.Marshal(flows[0])
	var cleanup map[string]any
	_ = json.Unmarshal(copyBytes, &cleanup)
	cleanup["id"] = "cleanup"
	cleanup["nodes"].([]any)[1].(map[string]any)["config"].(map[string]any)["path"] = "/cleanup"
	doc["workflows"] = append(flows, cleanup)
	lifecycle := doc["groups"].([]any)[0].(map[string]any)["lifecycle"].(map[string]any)
	lifecycle["cleanup"] = map[string]any{"workflowId": "cleanup"}
	lifecycle["cleanupTimeoutMs"] = cleanupTimeout
	source, err = json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	return source
}

func TestCancellationAndOwnerDisposalHaveDifferentCleanupPolicy(t *testing.T) {
	for _, owner := range []bool{false, true} {
		t.Run(map[bool]string{false: "cancel", true: "owner-close"}[owner], func(t *testing.T) {
			entered := make(chan struct{})
			var cleanupCalls atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/health" {
					close(entered)
					<-r.Context().Done()
					return
				}
				cleanupCalls.Add(1)
				_, _ = w.Write([]byte(`{"ok":true}`))
			}))
			defer server.Close()
			app, _ := runWorkspace(t, httpclient.Adapter{}, testCredentials{})
			h, err := app.Start(context.Background(), application.RunRequest{Source: lifecycleSource(t, server.URL, 100), Format: "json"})
			if err != nil {
				t.Fatal(err)
			}
			// Drain while waiting for the HTTP entry signal, then request the stop.
			done := make(chan application.RunReport, 1)
			go func() { done <- collectRun(t, h) }()
			select {
			case <-entered:
			case <-time.After(3 * time.Second):
				t.Fatal("HTTP did not start")
			}
			if owner {
				h.Close()
			} else if !app.Cancel(h.ID) {
				t.Fatal("active run missing")
			}
			report := <-done
			if report.Status != "cancelled" {
				t.Fatalf("%+v", report.Error)
			}
			if owner {
				if report.Cleanup.Status != "skipped" || report.Cleanup.Reason == nil || *report.Cleanup.Reason != "ownerClosed" || cleanupCalls.Load() != 0 {
					t.Fatal(report.Cleanup)
				}
			} else {
				if report.Cleanup.Status != "completed" || cleanupCalls.Load() != 1 {
					t.Fatal(report.Cleanup)
				}
			}
			if app.Cancel(h.ID) {
				t.Error("completed run remains registered")
			}
		})
	}
}

func TestCleanupDeadlineAndPrimaryErrorPrecedence(t *testing.T) {
	for _, primary := range []bool{false, true} {
		t.Run(map[bool]string{false: "cleanup-error", true: "primary-error"}[primary], func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/cleanup" {
					<-r.Context().Done()
					return
				}
				if primary {
					h, ok := w.(http.Hijacker)
					if !ok {
						t.Error("hijack unavailable")
						return
					}
					c, _, _ := h.Hijack()
					_ = c.Close()
					return
				}
				_, _ = w.Write([]byte(`null`))
			}))
			defer server.Close()
			app, logs := runWorkspace(t, httpclient.Adapter{}, testCredentials{})
			h, err := app.Start(context.Background(), application.RunRequest{Source: lifecycleSource(t, server.URL, 20), Format: "json"})
			if err != nil {
				t.Fatal(err)
			}
			report := collectRun(t, h)
			if report.Status != "error" || report.Cleanup.Error == nil || report.Cleanup.Error.Kind != "cleanupTimeout" {
				t.Fatalf("error=%+v cleanup=%+v", report.Error, report.Cleanup)
			}
			if primary && report.Error.Kind == "cleanupTimeout" {
				t.Fatal("cleanup replaced primary failure")
			}
			if !primary && report.Error.Kind != "cleanupTimeout" {
				t.Fatal(report.Error)
			}
			if !strings.Contains(logs.text(), "run failed") {
				t.Error("failure bypassed logging port")
			}
			if !primary {
				for _, e := range report.Events {
					if e.Type == "response.received" {
						body := e.Payload.(map[string]any)["body"].(map[string]any)
						if body["omitted"] != false || body["value"] != nil {
							t.Fatal("JSON null preview was omitted")
						}
					}
				}
			}
		})
	}
}

func TestPreparationAndBoundedDeliveryNeverLeaveUnboundedWork(t *testing.T) {
	t.Run("missing-credential", func(t *testing.T) {
		fake := &scriptedSessions{t: t}
		app, logs := runWorkspace(t, fake, testCredentials{missing: true})
		_, err := app.Start(context.Background(), application.RunRequest{Source: showcase(t, "json", "http://fixture.invalid", 1), Format: "json"})
		if err == nil || !strings.Contains(err.Error(), "missingSecret") || !fake.closed || strings.Contains(logs.text(), "run started") {
			t.Fatal("preparation spawned a run")
		}
	})
	t.Run("stalled-receiver", func(t *testing.T) {
		fake := &scriptedSessions{t: t}
		app, _ := runWorkspace(t, fake, testCredentials{})
		opts := application.DefaultRunOptions()
		opts.EventCapacity = 1
		h, err := app.Start(context.Background(), application.RunRequest{Source: showcase(t, "json", "http://fixture.invalid", 1), Format: "json", Options: opts})
		if err != nil {
			t.Fatal(err)
		}
		h.Close()
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		report, err := h.Wait(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if report.Error == nil || report.Error.Kind != "cancelled" || report.Summary.Requests != 0 || report.Cleanup.Status != "skipped" {
			t.Fatalf("unbounded/stalled run: %+v", report)
		}
	})
	t.Run("report-budget", func(t *testing.T) {
		fake := &scriptedSessions{t: t}
		app, _ := runWorkspace(t, fake, testCredentials{})
		opts := application.DefaultRunOptions()
		opts.MaxReportBytes = 131072
		h, err := app.Start(context.Background(), application.RunRequest{Source: showcase(t, "json", "http://fixture.invalid", 1), Format: "json", Options: opts})
		if err != nil {
			t.Fatal(err)
		}
		report := collectRun(t, h)
		data, _ := json.Marshal(report)
		if len(data) > opts.MaxReportBytes {
			t.Fatalf("report overflow: %d", len(data))
		}
		if report.Error != nil && report.Error.Kind != "resourceLimit" {
			t.Fatal(report.Error)
		}
	})
}
