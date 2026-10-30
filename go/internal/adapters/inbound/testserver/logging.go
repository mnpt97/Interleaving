package testserver

import (
	"net/http"
	"strconv"
	"sync/atomic"
	"time"

	"interleaving/internal/application"
)

// LogRequests measures handler time, including body reads, fixture waits and response
// writes. It excludes HTTP header parsing, final socket flushing and logger overhead.
// The logger is a port; the executable chooses the Charm adapter.
func LogRequests(logger application.Logger, next http.Handler) http.Handler {
	var sequence atomic.Uint64
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		started := time.Now()
		id := sequence.Add(1)
		response := &loggedResponse{ResponseWriter: w}
		returned := false
		defer func() {
			duration := time.Since(started)
			cancelled := r.Context().Err() != nil
			status := response.status
			if status == 0 && !cancelled && returned {
				status = http.StatusOK // net/http supplies 200 for an empty successful response.
			}
			logger.Log(r.Context(), application.Info, "HTTP request",
				application.Field{Name: "request_id", Value: strconv.FormatUint(id, 10)},
				application.Field{Name: "started_at", Value: started.UTC().Format(time.RFC3339Nano)},
				application.Field{Name: "method", Value: r.Method},
				application.Field{Name: "path", Value: r.URL.Path},
				application.Field{Name: "status", Value: strconv.Itoa(status)},
				application.Field{Name: "duration_ms", Value: strconv.FormatFloat(float64(duration.Nanoseconds())/1e6, 'f', 6, 64)},
				application.Field{Name: "response_bytes", Value: strconv.FormatInt(response.bytes, 10)},
				application.Field{Name: "cancelled", Value: strconv.FormatBool(cancelled)},
				application.Field{Name: "write_failed", Value: strconv.FormatBool(response.writeFailed)},
			)
		}()
		next.ServeHTTP(response, r)
		returned = true
	})
}

type loggedResponse struct {
	http.ResponseWriter
	status      int
	bytes       int64
	writeFailed bool
}

func (w *loggedResponse) WriteHeader(status int) {
	if w.status != 0 {
		return
	}
	w.ResponseWriter.WriteHeader(status)
	// Informational responses do not commit the final response status.
	if status >= 200 {
		w.status = status
	}
}

func (w *loggedResponse) Write(body []byte) (int, error) {
	if w.status == 0 {
		w.WriteHeader(http.StatusOK)
	}
	n, err := w.ResponseWriter.Write(body)
	w.bytes += int64(n)
	w.writeFailed = w.writeFailed || err != nil
	return n, err
}

// Unwrap lets http.ResponseController access capabilities of the original writer.
func (w *loggedResponse) Unwrap() http.ResponseWriter { return w.ResponseWriter }
