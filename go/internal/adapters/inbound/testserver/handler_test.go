package testserver_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/cookiejar"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"interleaving/internal/adapters/inbound/testserver"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/application"
	"interleaving/internal/bootstrap"
)

func fixture(t *testing.T) *httptest.Server {
	t.Helper()
	s := httptest.NewServer(testserver.NewHandler(scenariocodec.ResponseJSON{}))
	t.Cleanup(s.Close)
	return s
}
func request(t *testing.T, client *http.Client, method, url, body string, headers map[string]string) *http.Response {
	t.Helper()
	r, err := http.NewRequest(method, url, strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	for name, value := range headers {
		r.Header.Set(name, value)
	}
	response, err := client.Do(r)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { response.Body.Close() })
	return response
}
func value(t *testing.T, r *http.Response) map[string]any {
	t.Helper()
	defer r.Body.Close()
	var value map[string]any
	if err := json.NewDecoder(r.Body).Decode(&value); err != nil {
		t.Fatal(err)
	}
	return value
}

var jsonHeaders = map[string]string{"Content-Type": "application/json"}

func TestFakeAuthenticationIsFreshAndUserSessionsAreIndependent(t *testing.T) {
	s := fixture(t)
	jar, _ := cookiejar.New(nil)
	client := &http.Client{Jar: jar, Timeout: time.Second}
	var tokens []string
	for _, user := range []string{"alice", "bob"} {
		response := request(t, client, "POST", s.URL+"/auth/login", `{"username":"`+user+`","password":"`+user+`-password"}`, jsonHeaders)
		if response.StatusCode != 200 {
			t.Fatal(response.StatusCode)
		}
		cookie := response.Cookies()[0]
		if cookie.Name != testserver.CookieName || !cookie.HttpOnly || cookie.Path != "/" || cookie.SameSite != http.SameSiteLaxMode {
			t.Fatal("cookie contract changed")
		}
		login := value(t, response)
		token := login["accessToken"].(string)
		tokens = append(tokens, token)
		for _, auth := range []string{"bearer", "cookie"} {
			path := "/auth/cookie-me"
			headers := map[string]string{}
			if auth == "bearer" {
				path = "/auth/me"
				headers["Authorization"] = "bearer " + token
			}
			me := value(t, request(t, client, "GET", s.URL+path, "", headers))
			if me["user"] != user || me["sessionId"] != login["sessionId"] || me["accessToken"] != nil {
				t.Fatal("wrong authenticated session")
			}
		}
	}
	if tokens[0] == tokens[1] {
		t.Fatal("login did not issue fresh tokens")
	}
	if old := value(t, request(t, client, "GET", s.URL+"/auth/me", "", map[string]string{"Authorization": "Bearer " + tokens[0]})); old["user"] != "alice" {
		t.Fatal("old session was overwritten")
	}
	if response := request(t, client, "GET", s.URL+"/auth/me", "", nil); response.StatusCode != 401 {
		t.Fatal("cookie substituted for bearer")
	}
	for _, headers := range []http.Header{{"Authorization": {"Bearer " + tokens[0], "Bearer " + tokens[1]}}, {"Cookie": {"interl_session=one; interl_session=two"}}} {
		path := "/auth/me"
		if headers.Get("Cookie") != "" {
			path = "/auth/cookie-me"
		}
		r, _ := http.NewRequest("GET", s.URL+path, nil)
		r.Header = headers
		response, err := http.DefaultClient.Do(r)
		if err != nil {
			t.Fatal(err)
		}
		response.Body.Close()
		if response.StatusCode != 401 {
			t.Fatal("ambiguous credentials accepted")
		}
	}
}

func TestLoginVariantsAndRejectedInputs(t *testing.T) {
	s := fixture(t)
	for _, test := range []struct {
		body, content string
		status        int
	}{
		{`{"username":"alice","password":"private-marker"}`, "application/json", 401},
		{`{"username":"alice"}`, "application/json", 400}, {`{"username":"alice","password":null}`, "application/json", 400},
		{`{"username":"alice","password":"alice-password","extra":"private-marker"}`, "application/json", 400},
		{"{", "application/json", 400}, {"{}", "text/plain", 415}, {strings.Repeat("x", 4097), "application/json", 413},
	} {
		r := request(t, http.DefaultClient, "POST", s.URL+"/auth/login", test.body, map[string]string{"Content-Type": test.content})
		body, _ := io.ReadAll(r.Body)
		r.Body.Close()
		if r.StatusCode != test.status || strings.Contains(string(body), "private-marker") || len(r.Cookies()) != 0 {
			t.Fatalf("unsafe login rejection: %d", r.StatusCode)
		}
	}
	for _, variant := range []string{"missing-token", "malformed", "text", "unfamiliar", "large"} {
		r := request(t, http.DefaultClient, "POST", s.URL+"/auth/login-"+variant, `{"username":"alice","password":"alice-password"}`, jsonHeaders)
		body, _ := io.ReadAll(r.Body)
		r.Body.Close()
		if r.StatusCode != 200 || len(r.Cookies()) != 1 {
			t.Fatal("response variant did not establish a cookie")
		}
		var result map[string]any
		err := json.Unmarshal(body, &result)
		switch variant {
		case "malformed", "text":
			if err == nil || !strings.Contains(string(body), "fake-token-") {
				t.Fatal("invalid-body variant changed")
			}
		case "missing-token":
			if err != nil || result["accessToken"] != nil {
				t.Fatal("missing-token variant changed")
			}
		case "unfamiliar":
			if err != nil || result["opaqueArtifact"] == nil || result["accessToken"] != nil {
				t.Fatal("unfamiliar-token variant changed")
			}
		case "large":
			if err != nil || len(result["padding"].(string)) != 131072 {
				t.Fatal("large-body variant changed")
			}
		}
	}
}

func TestLongPollBoundsTimingAndHTTPRoutes(t *testing.T) {
	s := fixture(t)
	for _, body := range []string{`{}`, `{"timeoutMs":-1}`, `{"timeoutMs":1.5}`, `{"timeoutMs":null}`, `{"timeoutMs":60001}`, `{"timeoutMs":0,"extra":"private-marker"}`, `{"timeoutMs":0,"timeoutMs":1}`, `{"timeoutMs":"private-marker"}`} {
		r := request(t, http.DefaultClient, "POST", s.URL+"/long-poll", body, jsonHeaders)
		reply, _ := io.ReadAll(r.Body)
		r.Body.Close()
		if r.StatusCode != 400 || strings.Contains(string(reply), "private-marker") {
			t.Fatal("invalid wait accepted or echoed")
		}
	}
	for _, test := range []struct {
		method, body, content string
		status                int
	}{{"GET", "", "", 405}, {"POST", `{"timeoutMs":0}`, "", 415}, {"POST", strings.Repeat("x", 1025), "application/json", 413}} {
		r := request(t, http.DefaultClient, test.method, s.URL+"/long-poll", test.body, map[string]string{"Content-Type": test.content})
		r.Body.Close()
		if r.StatusCode != test.status {
			t.Fatal(r.StatusCode)
		}
	}
	for _, delay := range []int{0, 25} {
		body, _ := json.Marshal(map[string]int{"timeoutMs": delay})
		started := time.Now()
		r := request(t, http.DefaultClient, "POST", s.URL+"/long-poll", string(body), jsonHeaders)
		v := value(t, r)
		if r.StatusCode != 200 || v["timeoutMs"] != float64(delay) || time.Since(started) < time.Duration(delay)*time.Millisecond {
			t.Fatal("wait duration or result changed")
		}
	}
	if v := value(t, request(t, http.DefaultClient, "GET", s.URL+"/routing-body", "", nil)); v["value"].(map[string]any)["status"] != true {
		t.Fatal("routing fixture changed")
	}
	if r := request(t, http.DefaultClient, "GET", s.URL+"/not-found", "", nil); r.StatusCode != 404 {
		t.Fatal("not-found fixture changed")
	}
	echo := value(t, request(t, http.DefaultClient, "POST", s.URL+"/echo", `{"value":true}`, map[string]string{"X-Test": "example"}))
	if echo["method"] != "POST" || echo["body"].(map[string]any)["value"] != true || echo["headers"].(map[string]any)["x-test"] != "example" {
		t.Fatal("echo fixture changed")
	}
}

func TestCancelledWaitDoesNotBlockOtherRequestsOrShutdown(t *testing.T) {
	s := fixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	r, _ := http.NewRequestWithContext(ctx, "POST", s.URL+"/long-poll", strings.NewReader(`{"timeoutMs":60000}`))
	r.Header.Set("Content-Type", "application/json")
	done := make(chan struct{})
	go func() {
		defer close(done)
		response, _ := http.DefaultClient.Do(r)
		if response != nil {
			response.Body.Close()
		}
	}()
	if response := request(t, &http.Client{Timeout: time.Second}, "GET", s.URL+"/health", "", nil); response.StatusCode != 200 {
		t.Fatal("wait blocked health")
	}
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("wait did not cancel")
	}
}

func TestFeatureShowcaseRunsAgainstGoServerInBothFormatsAndBranches(t *testing.T) {
	t.Setenv("INTERL_ALICE_USERNAME", "alice")
	t.Setenv("INTERL_ALICE_PASSWORD", "alice-password")
	for _, format := range []string{"json", "yaml"} {
		for _, fallback := range []bool{false, true} {
			t.Run(format+map[bool]string{false: "/accepted", true: "/fallback"}[fallback], func(t *testing.T) {
				s := fixture(t)
				source, err := os.ReadFile("../../../../../scenarios/examples/feature-showcase.interl." + format)
				if err != nil {
					t.Fatal(err)
				}
				source = bytes.ReplaceAll(source, []byte("http://127.0.0.1:8080"), []byte(s.URL))
				if format == "yaml" {
					source = bytes.ReplaceAll(source, []byte("waitMs: 2000"), []byte("waitMs: 0"))
					if fallback {
						source = bytes.ReplaceAll(source, []byte("value: 1"), []byte("value: 4"))
					}
				} else {
					var doc map[string]any
					_ = json.Unmarshal(source, &doc)
					doc["environment"].(map[string]any)["variables"].(map[string]any)["waitMs"] = 0
					if fallback {
						doc["groups"].([]any)[0].(map[string]any)["lifecycle"].(map[string]any)["iteration"].(map[string]any)["inputs"].(map[string]any)["minimum"].(map[string]any)["value"] = 4
					}
					source, _ = json.Marshal(doc)
				}
				var logs bytes.Buffer
				app := bootstrap.Workspace(charmlog.New(&logs))
				h, err := app.Start(context.Background(), application.RunRequest{Source: source, Format: format})
				if err != nil {
					t.Fatal(err)
				}
				defer h.Close()
				for range h.Events {
				}
				report, err := h.Wait(context.Background())
				if err != nil {
					t.Fatal(err)
				}
				if report.Status != "passed" || report.Cleanup.Status != "completed" || report.Summary.Requests != 12 || report.Summary.Responses != 12 || report.Summary.HTTPErrorResponses != 0 {
					t.Fatalf("bad showcase: %+v error=%+v", report.Summary, report.Error)
				}
				wire, _ := json.Marshal(report)
				for _, secret := range []string{"alice-password", "fake-token-", "fake-cookie-"} {
					if bytes.Contains(wire, []byte(secret)) || strings.Contains(logs.String(), secret) {
						t.Fatal("credential leaked into report/logs")
					}
				}
			})
		}
	}
}

func TestSessionStorageIsBoundedUnderConcurrentLogins(t *testing.T) {
	h := testserver.NewHandler(scenariocodec.ResponseJSON{})
	var workers sync.WaitGroup
	var accepted, rejected atomic.Int32
	for i := 0; i < 16; i++ {
		workers.Add(1)
		go func() {
			defer workers.Done()
			for j := 0; j < 65; j++ {
				r := httptest.NewRequest("POST", "/auth/login", strings.NewReader(`{"username":"alice","password":"alice-password"}`))
				r.Header.Set("Content-Type", "application/json")
				w := httptest.NewRecorder()
				h.ServeHTTP(w, r)
				switch w.Code {
				case 200:
					accepted.Add(1)
				case 503:
					rejected.Add(1)
				default:
					t.Errorf("unexpected login status %d", w.Code)
				}
			}
		}()
	}
	workers.Wait()
	if accepted.Load() != 1024 || rejected.Load() != 16 {
		t.Fatalf("capacity race: %d accepted, %d rejected", accepted.Load(), rejected.Load())
	}
	// Capacity is shared across concurrent requests and excess logins fail closed.
	r := httptest.NewRequest("POST", "/auth/login", strings.NewReader(`{"username":"alice","password":"alice-password"}`))
	r.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	h.ServeHTTP(w, r)
	if w.Code != 503 || len(w.Result().Cookies()) != 0 {
		t.Fatal("session storage exceeded its cap")
	}
}
