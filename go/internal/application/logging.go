package application

import "context"

// Level is application-owned; concrete logger levels never leak into the core.
type Level uint8

const (
	Debug Level = iota
	Info
	Warn
	Error
)

// Field contains safe diagnostic metadata, never credentials or raw HTTP payloads.
type Field struct {
	Name  string
	Value string
}

// Logger is a driven port. Logging must not terminate a process or emit run events.
// Callers must sanitize sensitive data before this boundary, just as for reports.
type Logger interface {
	Log(context.Context, Level, string, ...Field)
}
