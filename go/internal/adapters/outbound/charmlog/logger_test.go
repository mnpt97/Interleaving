package charmlog_test

import (
	"bytes"
	"context"
	"strings"
	"testing"

	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/application"
)

func TestAdapterWritesOnlyToInjectedDiagnosticSink(t *testing.T) {
	var output bytes.Buffer
	var logger application.Logger = charmlog.New(&output)
	logger.Log(context.Background(), application.Info, "bridge ready", application.Field{Name: "component", Value: "desktop"}, application.Field{Name: "password", Value: "fake-secret-sentinel"})
	text := output.String()
	if !strings.Contains(text, "bridge ready") || !strings.Contains(text, "desktop") {
		t.Fatal("missing structured diagnostic")
	}
	if strings.Contains(text, "fake-secret-sentinel") {
		t.Fatal("credential field leaked")
	}
}
