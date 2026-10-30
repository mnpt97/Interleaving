package testserver_test

import (
	"context"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"

	"interleaving/internal/adapters/inbound/testserver"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/application"
)

type logFunc func(context.Context, application.Level, string, ...application.Field)

func (f logFunc) Log(ctx context.Context, level application.Level, message string, fields ...application.Field) {
	f(ctx, level, message, fields...)
}

func captureRequestLog() (application.Logger, *map[string]string, *int) {
	var last map[string]string
	count := 0
	logger := logFunc(func(_ context.Context, _ application.Level, _ string, fields ...application.Field) {
		count++
		last = make(map[string]string)
		for _, field := range fields {
			last[field.Name] = field.Value
		}
	})
	return logger, &last, &count
}

func TestRequestLogsMeasureWaitAndPreserveResponse(t *testing.T) {
	logger, logged, count := captureRequestLog()
	h := testserver.LogRequests(logger, testserver.NewHandler(scenariocodec.ResponseJSON{}))
	r := httptest.NewRequest("POST", "/long-poll?secret=private-marker", strings.NewReader(`{"timeoutMs":25}`))
	r.Header.Set("Content-Type", "application/json")
	r.Header.Set("Authorization", "Bearer private-marker")
	w := httptest.NewRecorder()
	start := time.Now()
	h.ServeHTTP(w, r)
	elapsed := time.Since(start)
	fields := *logged
	if *count != 1 || w.Code != 200 || !strings.Contains(w.Body.String(), `"timeoutMs":25`) {
		t.Fatal("logging changed the response or did not log exactly once")
	}
	if fields["method"] != "POST" || fields["path"] != "/long-poll" || fields["status"] != "200" || fields["response_bytes"] != strconv.Itoa(w.Body.Len()) {
		t.Fatalf("incorrect request metadata: %+v", fields)
	}
	ms, err := strconv.ParseFloat(fields["duration_ms"], 64)
	if err != nil {
		t.Fatal(err)
	}
	if ms < 25 || ms > float64(elapsed.Nanoseconds())/1e6 {
		t.Fatalf("duration %v ms does not include the wait or exceeds the outer measurement", ms)
	}
	if _, err := time.Parse(time.RFC3339Nano, fields["started_at"]); err != nil {
		t.Fatal(err)
	}
	// Only explicit metadata is forwarded: never bodies, query strings or credentials.
	if len(fields) != 9 || fields["cancelled"] != "false" || fields["write_failed"] != "false" || fields["request_id"] != "1" {
		t.Fatalf("unexpected log fields: %+v", fields)
	}
}

func TestRequestLogsIncludeRejectedAndUnmatchedRoutes(t *testing.T) {
	logger, logged, count := captureRequestLog()
	h := testserver.LogRequests(logger, testserver.NewHandler(scenariocodec.ResponseJSON{}))
	for index, test := range []struct {
		method, path string
		status       int
	}{{"GET", "/health", 200}, {"GET", "/not-found", 404}, {"GET", "/missing", 404}, {"GET", "/long-poll", 405}, {"GET", "/auth/me", 401}, {"POST", "/auth/login", 415}} {
		w := httptest.NewRecorder()
		h.ServeHTTP(w, httptest.NewRequest(test.method, test.path, nil))
		if w.Code != test.status || (*logged)["status"] != strconv.Itoa(test.status) || (*logged)["response_bytes"] != strconv.Itoa(w.Body.Len()) || (*logged)["request_id"] != strconv.Itoa(index+1) {
			t.Fatalf("incorrect response/log for %s: %+v", test.path, *logged)
		}
	}
	if *count != 6 {
		t.Fatal("missing or duplicated request logs")
	}
}

func TestRequestLogsMarkCancelledWaitWithoutClaimingHTTP200(t *testing.T) {
	logger, logged, count := captureRequestLog()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	// Cancel after successful parsing, so the request reaches the wait rather than
	// failing body validation because its context was already cancelled.
	h := testserver.LogRequests(logger, testserver.NewHandler(cancelAfterParsing{cancel: cancel}))
	r := httptest.NewRequest("POST", "/long-poll", strings.NewReader(`{"timeoutMs":60000}`)).WithContext(ctx)
	r.Header.Set("Content-Type", "application/json")
	h.ServeHTTP(httptest.NewRecorder(), r)
	if *count != 1 || (*logged)["cancelled"] != "true" || (*logged)["status"] != "0" || (*logged)["response_bytes"] != "0" {
		t.Fatalf("cancelled request misreported: %+v", *logged)
	}
}

type cancelAfterParsing struct{ cancel context.CancelFunc }

func (p cancelAfterParsing) Parse(ctx context.Context, body []byte) (any, *application.Failure) {
	value, failure := (scenariocodec.ResponseJSON{}).Parse(ctx, body)
	p.cancel()
	return value, failure
}
