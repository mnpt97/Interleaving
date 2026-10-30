// Package testserver provides the loopback HTTP fixture for manual app testing.
// Its predictable credentials and sessions are deliberately fake.
package testserver

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
	"unicode"

	"interleaving/internal/application"
)

const CookieName = "interl_session"
const maxSessions = 1024

type session struct{ user, id string }
type handler struct {
	json    application.ResponseJSON
	mu      sync.Mutex
	next    uint64
	tokens  map[string]session
	cookies map[string]string
}

// NewHandler creates isolated fixture state. Recreating it clears all sessions.
// ResponseJSON supplies bounded, duplicate-aware parsing through the existing port.
func NewHandler(parser application.ResponseJSON) http.Handler {
	h := &handler{json: parser, tokens: map[string]session{}, cookies: map[string]string{}}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, r *http.Request) { writeJSON(w, 200, map[string]any{"status": "ok"}) })
	mux.HandleFunc("GET /routing-body", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, 200, map[string]any{"value": map[string]any{"status": true, "attempts": 3, "label": "ready"}})
	})
	mux.HandleFunc("GET /not-found", func(w http.ResponseWriter, r *http.Request) { failure(w, 404, "not found") })
	mux.HandleFunc("GET /slow", func(w http.ResponseWriter, r *http.Request) {
		if wait(r.Context(), 10*time.Second) {
			writeJSON(w, 200, map[string]any{"status": "late"})
		}
	})
	mux.HandleFunc("GET /echo", h.echo)
	mux.HandleFunc("POST /echo", h.echo)
	mux.HandleFunc("POST /long-poll", h.longPoll)
	for _, mode := range []string{"login", "login-missing-token", "login-malformed", "login-text", "login-unfamiliar", "login-slow", "login-large"} {
		mux.HandleFunc("POST /auth/"+mode, func(w http.ResponseWriter, r *http.Request) { h.login(w, r, mode) })
	}
	mux.HandleFunc("GET /auth/me", h.bearerMe)
	mux.HandleFunc("GET /auth/cookie-me", h.cookieMe)
	return mux
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	var body bytes.Buffer
	encoder := json.NewEncoder(&body)
	encoder.SetEscapeHTML(false)
	if encoder.Encode(value) != nil {
		w.WriteHeader(http.StatusInternalServerError)
		return
	}
	wire := bytes.TrimSuffix(body.Bytes(), []byte{'\n'})
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Content-Length", fmt.Sprint(len(wire)))
	w.WriteHeader(status)
	_, _ = w.Write(wire)
}
func failure(w http.ResponseWriter, status int, message string) {
	writeJSON(w, status, map[string]any{"error": message})
}
func wait(ctx context.Context, delay time.Duration) bool {
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return ctx.Err() == nil
	}
}
func readBody(w http.ResponseWriter, r *http.Request, limit int64) ([]byte, bool) {
	defer r.Body.Close()
	body, err := io.ReadAll(io.LimitReader(r.Body, limit+1))
	if len(body) > int(limit) {
		failure(w, 413, "request body too large")
		return nil, false
	}
	if err != nil {
		failure(w, 400, "invalid request body")
		return nil, false
	}
	return body, true
}
func jsonContent(r *http.Request) bool {
	return strings.EqualFold(strings.TrimSpace(strings.SplitN(r.Header.Get("Content-Type"), ";", 2)[0]), "application/json")
}

func (h *handler) echo(w http.ResponseWriter, r *http.Request) {
	body, ok := readBody(w, r, 2*1024*1024)
	if !ok {
		return
	}
	var value any
	// Echo accepts arbitrary bodies; invalid/non-JSON text is represented as null.
	decoder := json.NewDecoder(strings.NewReader(string(body)))
	decoder.UseNumber()
	if decoder.Decode(&value) != nil || decoder.Decode(new(any)) != io.EOF {
		value = nil
	}
	headers := map[string]string{}
	for name, values := range r.Header {
		if len(values) > 0 {
			headers[strings.ToLower(name)] = values[len(values)-1]
		}
	}
	if r.Host != "" {
		headers["host"] = r.Host
	}
	writeJSON(w, 200, map[string]any{"method": r.Method, "headers": headers, "body": value})
}
func (h *handler) longPoll(w http.ResponseWriter, r *http.Request) {
	body, ok := readBody(w, r, 1024)
	if !ok {
		return
	}
	if !jsonContent(r) {
		failure(w, 415, "Use Content-Type: application/json.")
		return
	}
	value, err := h.json.Parse(r.Context(), body)
	fields, object := value.(map[string]any)
	ms, integer := fields["timeoutMs"].(json.Number)
	timeout, parseErr := ms.Int64()
	if err != nil || !object || len(fields) != 1 || !integer || parseErr != nil || timeout < 0 || timeout > 60000 {
		failure(w, 400, "timeoutMs must be an integer from 0 to 60000.")
		return
	}
	if wait(r.Context(), time.Duration(timeout)*time.Millisecond) {
		writeJSON(w, 200, map[string]any{"status": "complete", "timeoutMs": timeout})
	}
}

func (h *handler) login(w http.ResponseWriter, r *http.Request, mode string) {
	body, ok := readBody(w, r, 4096)
	if !ok {
		return
	}
	if !jsonContent(r) {
		failure(w, 415, "application/json required")
		return
	}
	value, err := h.json.Parse(r.Context(), body)
	fields, object := value.(map[string]any)
	if err != nil || !object {
		failure(w, 400, "invalid login shape")
		return
	}
	username, userOK := fields["username"].(string)
	password, passOK := fields["password"].(string)
	if !userOK || !passOK {
		failure(w, 400, "username and password strings required")
		return
	}
	if len(fields) != 2 {
		failure(w, 400, "unexpected login fields")
		return
	}
	if !(username == "alice" && password == "alice-password" || username == "bob" && password == "bob-password") {
		failure(w, 401, "invalid credentials")
		return
	}
	if mode == "login-slow" && !wait(r.Context(), 10*time.Second) {
		return
	}
	if r.Context().Err() != nil {
		return
	}
	h.mu.Lock()
	if len(h.tokens) >= maxSessions {
		h.mu.Unlock()
		failure(w, 503, "fixture session capacity reached; restart server")
		return
	}
	h.next++
	token := fmt.Sprintf("fake-token-%08d", h.next)
	cookie := fmt.Sprintf("fake-cookie-%08d", h.next)
	s := session{user: username, id: fmt.Sprintf("fake-session-%08d", h.next)}
	h.tokens[token] = s
	h.cookies[cookie] = token
	h.mu.Unlock()
	http.SetCookie(w, &http.Cookie{Name: CookieName, Value: cookie, Path: "/", HttpOnly: true, SameSite: http.SameSiteLaxMode})
	switch mode {
	case "login-malformed":
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"accessToken":"`+token)
	case "login-text":
		w.Header().Set("Content-Type", "text/plain")
		_, _ = io.WriteString(w, token)
	default:
		result := map[string]any{"user": s.user, "sessionId": s.id}
		switch mode {
		case "login-missing-token":
		case "login-unfamiliar":
			result["opaqueArtifact"] = token
		default:
			result["accessToken"] = token
		}
		if mode == "login-large" {
			result["padding"] = strings.Repeat("x", 131072)
		}
		writeJSON(w, 200, result)
	}
}
func (h *handler) bearerMe(w http.ResponseWriter, r *http.Request) {
	values := r.Header.Values("Authorization")
	if len(values) != 1 {
		failure(w, 401, "unauthorized")
		return
	}
	scheme, token, ok := strings.Cut(values[0], " ")
	if !ok || !strings.EqualFold(scheme, "Bearer") || token == "" || strings.IndexFunc(token, unicode.IsSpace) >= 0 {
		failure(w, 401, "unauthorized")
		return
	}
	h.mu.Lock()
	s, found := h.tokens[token]
	h.mu.Unlock()
	sessionResponse(w, s, found)
}
func (h *handler) cookieMe(w http.ResponseWriter, r *http.Request) {
	value := ""
	foundCookie := false
	for _, header := range r.Header.Values("Cookie") {
		for _, pair := range strings.Split(header, ";") {
			name, v, ok := strings.Cut(strings.TrimSpace(pair), "=")
			if ok && name == CookieName {
				if foundCookie {
					failure(w, 401, "unauthorized")
					return
				}
				foundCookie = true
				value = v
			}
		}
	}
	h.mu.Lock()
	token := h.cookies[value]
	s, found := h.tokens[token]
	h.mu.Unlock()
	sessionResponse(w, s, foundCookie && found)
}
func sessionResponse(w http.ResponseWriter, s session, found bool) {
	if !found {
		failure(w, 401, "unauthorized")
		return
	}
	writeJSON(w, 200, map[string]any{"user": s.user, "sessionId": s.id})
}
