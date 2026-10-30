package application

import (
	"context"
	"errors"

	"interleaving/internal/domain"
)

// ScenarioCodec is the driven port for strict external representation admission.
// Domain types and safe diagnostics cross the boundary; YAML/schema types do not.
type ScenarioCodec interface {
	Decode(context.Context, []byte, string) (*domain.Document, error)
}

// ScenarioFiles owns bounded filesystem reads and extension selection. Validation
// itself does not know the filesystem or the operating system's environment.
type ScenarioFiles interface {
	Read(context.Context, string) ([]byte, string, error)
}
type ScenarioServices struct {
	Codec       ScenarioCodec
	Files       ScenarioFiles
	Expressions domain.Expressions
	Targets     domain.TargetURLs
	Writer      ScenarioWriter
}

// ScenarioWriter persists admitted JSON drafts; execution validity is separate.
type ScenarioWriter interface {
	WriteJSON(context.Context, string, []byte) error
}

func (w *Workspace) Validate(ctx context.Context, source, format string) error {
	if w.scenarios == nil {
		return domain.ErrUnavailable
	}
	_, err := w.Compile(ctx, []byte(source), format)
	return err
}

// Compile admits one source, validates every definition, and returns a checked plan.
// No executable engine is involved and no secrets are resolved during this use case.
func (w *Workspace) Compile(ctx context.Context, source []byte, format string) (*domain.Program, error) {
	if w.scenarios == nil {
		return nil, domain.ErrUnavailable
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	doc, err := w.scenarios.Codec.Decode(ctx, source, format)
	if err != nil {
		return nil, err
	}
	program, diagnostics := domain.Compile(ctx, doc, w.scenarios.Expressions, w.scenarios.Targets)
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if len(diagnostics) > 0 {
		return nil, diagnostics
	}
	return program, nil
}
func (w *Workspace) ValidateFile(ctx context.Context, path string) error {
	if w.scenarios == nil || w.scenarios.Files == nil {
		return domain.ErrUnavailable
	}
	source, format, err := w.scenarios.Files.Read(ctx, path)
	if err == nil {
		_, err = w.Compile(ctx, source, format)
	}
	var diagnostics domain.Diagnostics
	var diagnostic domain.Diagnostic
	if errors.As(err, &diagnostics) {
		for i := range diagnostics {
			diagnostics[i].FilePath = &path
		}
		return diagnostics
	}
	if errors.As(err, &diagnostic) {
		diagnostic.FilePath = &path
		return diagnostic
	}
	return err
}

// WithScenarios supplies replaceable admission, file, and expression adapters.
func WithScenarios(services ScenarioServices) func(*Workspace) {
	return func(w *Workspace) { w.scenarios = &services }
}
