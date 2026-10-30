package httpclient_test

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"interleaving/internal/adapters/outbound/httpclient"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

func newSession(t *testing.T) application.Session {
	t.Helper()
	s, err := (httpclient.Adapter{}).New(domain.ClientProfile{TimeoutMS: 1000, Headers: map[string]string{"x-priority": "profile", "x-remove": "profile"}}, application.DefaultRunOptions())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	return s
}

func TestFreshAuthenticationDropsInheritedCredentialsWithoutMutatingProfile(t *testing.T) {
	profile := domain.ClientProfile{TimeoutMS: 1000, Headers: map[string]string{"Authorization": "old", "Cookie": "old=old", "Proxy-Authorization": "old", "x-default": "kept"}}
	s, err := (httpclient.Adapter{}).New(profile, application.DefaultRunOptions())
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err = s.Reset(); err != nil {
		t.Fatal(err)
	}
	r, err := s.Prepare(application.HTTPInput{Method: "GET", BaseURL: "http://fixture.test", Path: "/health"})
	if err != nil {
		t.Fatal(err)
	}
	if r.Headers["authorization"] != "" || r.Headers["cookie"] != "" || r.Headers["proxy-authorization"] != "" || r.Headers["x-default"] != "kept" || profile.Headers["Authorization"] != "old" {
		t.Fatal("fresh authentication retained credentials or mutated the profile")
	}
}

func TestInvalidTLSPeerAndIncompleteBodiesHaveSafeCategories(t *testing.T) {
	tls := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {}))
	defer tls.Close()
	s := newSession(t)
	r, err := s.Prepare(application.HTTPInput{Method: "GET", BaseURL: tls.URL, Path: "/"})
	if err != nil {
		t.Fatal(err)
	}
	_, err = s.Do(context.Background(), r)
	if err == nil || err.Kind != "tls" || strings.Contains(err.Message, tls.URL) {
		t.Fatal(err)
	}
	broken := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, buf, err := w.(http.Hijacker).Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		_, _ = fmt.Fprint(buf, "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
		_ = buf.Flush()
		_ = conn.Close()
	}))
	defer broken.Close()
	r, err = s.Prepare(application.HTTPInput{Method: "GET", BaseURL: broken.URL, Path: "/"})
	if err != nil {
		t.Fatal(err)
	}
	_, err = s.Do(context.Background(), r)
	if err == nil || err.Kind != "body" || strings.Contains(err.Message, broken.URL) {
		t.Fatal(err)
	}
}

func TestPathsAndHeaderPrecedence(t *testing.T) {
	s := newSession(t)
	if err := s.UpdateHeaders(map[string]string{"X-Priority": "session"}, []string{"X-Remove"}); err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{"//evil.test/", "/../escape", "/%2e%2e/escape", "/health#fragment", "/health\r\n"} {
		if _, err := s.Prepare(application.HTTPInput{BaseURL: "http://fixture.test/api", Method: "GET", Path: path}); err == nil {
			t.Errorf("accepted unsafe path %q", path)
		}
	}
	r, err := s.Prepare(application.HTTPInput{BaseURL: "http://fixture.test/api", Method: "GET", Path: "/health?value=a%2Fb", Headers: map[string]string{"x-priority": "node"}})
	if err != nil || r.URL != "http://fixture.test/api/health?value=a%2Fb" || r.Headers["x-priority"] != "node" || r.Headers["x-remove"] != "" {
		t.Fatalf("%+v %+v", r, err)
	}
	if err = s.UpdateHeaders(map[string]string{"x-good": "committed?", "x-bad": "\r\n"}, nil); err == nil {
		t.Fatal("invalid update accepted")
	}
	r, err = s.Prepare(application.HTTPInput{BaseURL: "http://fixture.test/api", Method: "GET", Path: "/health"})
	if err != nil || r.Headers["x-good"] != "" || r.Headers["x-priority"] != "session" {
		t.Fatal("update was not atomic")
	}
}

func TestCookieIsolationResetAndRedirectPolicy(t *testing.T) {
	redirected := false
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/login":
			http.SetCookie(w, &http.Cookie{Name: "sid", Value: "fake-cookie", Path: "/"})
		case "/redirect":
			w.Header().Set("Location", "/target")
			w.WriteHeader(302)
		case "/target":
			redirected = true
		case "/cookie":
			_, _ = io.WriteString(w, r.Header.Get("Cookie"))
		}
	}))
	defer server.Close()
	a, b := newSession(t), newSession(t)
	call := func(s application.Session, path string) (application.Request, application.Response) {
		t.Helper()
		r, err := s.Prepare(application.HTTPInput{BaseURL: server.URL, Path: path, Method: "GET"})
		if err != nil {
			t.Fatal(err)
		}
		v, err := s.Do(context.Background(), r)
		if err != nil {
			t.Fatal(err)
		}
		return r, v
	}
	r, response := call(a, "/login")
	if !a.HasResponseCookies(r.URL, response) {
		t.Fatal("response cookie not recognized")
	}
	_, response = call(a, "/cookie")
	if !strings.Contains(string(response.Body), "sid=fake-cookie") {
		t.Fatal("session cookie missing")
	}
	_, response = call(b, "/cookie")
	if len(response.Body) != 0 {
		t.Fatal("cookie crossed run boundary")
	}
	if err := a.Reset(); err != nil {
		t.Fatal(err)
	}
	_, response = call(a, "/cookie")
	if len(response.Body) != 0 {
		t.Fatal("fresh authentication retained cookie")
	}
	_, response = call(a, "/redirect")
	if response.Status != 302 || redirected {
		t.Fatal("redirect followed")
	}
}

func TestBoundedResponseAndTimeout(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/slow" {
			<-r.Context().Done()
			return
		}
		_, _ = io.WriteString(w, strings.Repeat("x", 128))
	}))
	defer server.Close()
	opts := application.DefaultRunOptions()
	opts.MaxResponseBytes = 16
	s, err := (httpclient.Adapter{}).New(domain.ClientProfile{TimeoutMS: 20}, opts)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	r, err := s.Prepare(application.HTTPInput{BaseURL: server.URL, Path: "/large", Method: "GET"})
	if err != nil {
		t.Fatal(err)
	}
	response, err := s.Do(context.Background(), r)
	if err != nil || !response.Truncated || len(response.Body) != 16 {
		t.Fatalf("%+v %+v", response, err)
	}
	r, err = s.Prepare(application.HTTPInput{BaseURL: server.URL, Path: "/slow", Method: "GET"})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = s.Do(ctx, r)
	if err == nil || err.Kind != "timeout" || strings.Contains(err.Message, server.URL) {
		t.Fatalf("unsafe/misclassified error: %+v", err)
	}
}
