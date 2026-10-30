// Package charmlog implements the logging port with charmbracelet/log.
package charmlog

import (
	"context"
	"io"
	"log/slog"
	"strings"

	charm "charm.land/log/v2"

	"interleaving/internal/application"
)

// Logger uses an instance, never the Charm package-global logger. Output is injected at composition.
type Logger struct{ native *charm.Logger }

func New(output io.Writer) *Logger {
	return &Logger{native: charm.NewWithOptions(output, charm.Options{ReportTimestamp: true})}
}
func (l *Logger) Log(_ context.Context, level application.Level, message string, fields ...application.Field) {
	values := make([]any, 0, len(fields)*2)
	for _, field := range fields {
		value := field.Value
		// Defense in depth for well-known keys; this does not replace core redaction of arbitrary values.
		switch strings.ToLower(field.Name) {
		case "password", "authorization", "cookie", "set-cookie", "token":
			value = "[omitted]"
		}
		values = append(values, field.Name, value)
	}
	switch level {
	case application.Debug:
		l.native.Debug(message, values...)
	case application.Warn:
		l.native.Warn(message, values...)
	case application.Error:
		l.native.Error(message, values...)
	default:
		l.native.Info(message, values...)
	}
}

// Slog adapts framework-only logs without exposing the framework API to the application.
func (l *Logger) Slog() *slog.Logger { return slog.New(l.native) }

var _ application.Logger = (*Logger)(nil)
