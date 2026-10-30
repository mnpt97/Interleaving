// Command interl-test-server runs the local HTTP/auth fixture without Wails or Rust.
package main

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"interleaving/internal/adapters/inbound/testserver"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/application"
)

func main() { os.Exit(run()) }
func run() int {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	logger := charmlog.New(os.Stderr)
	listener, err := net.Listen("tcp", "127.0.0.1:8080")
	if err != nil {
		logger.Log(ctx, application.Error, "Cannot start test server; port 8080 may already be in use")
		return 3
	}
	server := &http.Server{Handler: testserver.LogRequests(logger, testserver.NewHandler(scenariocodec.ResponseJSON{})), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 5 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 65536, ErrorLog: slog.NewLogLogger(logger.Slog().Handler(), slog.LevelError), BaseContext: func(net.Listener) context.Context { return ctx }}
	stopped := make(chan struct{})
	go func() {
		defer close(stopped)
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if server.Shutdown(shutdown) != nil {
			_ = server.Close()
		}
	}()
	logger.Log(ctx, application.Info, "Local test server listening", application.Field{Name: "url", Value: "http://127.0.0.1:8080"})
	err = server.Serve(listener)
	stop()
	<-stopped
	if err != nil && !errors.Is(err, http.ErrServerClosed) {
		logger.Log(context.Background(), application.Error, "Test server stopped unexpectedly")
		return 3
	}
	logger.Log(context.Background(), application.Info, "Local test server stopped")
	return 0
}
