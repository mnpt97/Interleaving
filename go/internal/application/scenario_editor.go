package application

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"interleaving/internal/domain"
	"unicode/utf8"
)

type ValidationResult struct {
	Valid       bool               `json:"valid"`
	Document    any                `json:"document"`
	Diagnostics domain.Diagnostics `json:"diagnostics"`
}
type OpenResult struct {
	Path       string           `json:"path"`
	Format     string           `json:"format"`
	Source     string           `json:"source"`
	Validation ValidationResult `json:"validation"`
}

// Diagnostics returns safe structured failures, never raw OS/framework errors.
func Diagnostics(err error) domain.Diagnostics {
	if err == nil {
		return domain.Diagnostics{}
	}
	var many domain.Diagnostics
	var one domain.Diagnostic
	if errors.As(err, &many) {
		return many
	}
	if errors.As(err, &one) {
		return domain.Diagnostics{one}
	}
	return domain.Diagnostics{domain.Problem("io", "operationFailed", "$", "The operation could not be completed. Check the file and permissions.")}
}

// Inspect retains structurally admitted drafts even when their graphs cannot run.
func (w *Workspace) Inspect(ctx context.Context, source, format string) ValidationResult {
	result := ValidationResult{Diagnostics: domain.Diagnostics{}}
	if w.scenarios == nil {
		result.Diagnostics = Diagnostics(domain.ErrUnavailable)
		return result
	}
	doc, err := w.scenarios.Codec.Decode(ctx, []byte(source), format)
	if err != nil {
		result.Diagnostics = Diagnostics(err)
		return result
	}
	result.Document, err = admittedWire(doc)
	if err != nil {
		result.Diagnostics = Diagnostics(err)
		return result
	}
	_, result.Diagnostics = domain.Compile(ctx, doc, w.scenarios.Expressions, w.scenarios.Targets)
	if ctx.Err() != nil {
		result.Document = nil
		result.Diagnostics = Diagnostics(ctx.Err())
	}
	if result.Diagnostics == nil {
		result.Diagnostics = domain.Diagnostics{}
	}
	result.Valid = len(result.Diagnostics) == 0
	return result
}
func (w *Workspace) OpenScenario(ctx context.Context, path string) (OpenResult, error) {
	if w.scenarios == nil || w.scenarios.Files == nil {
		return OpenResult{}, domain.ErrUnavailable
	}
	source, format, err := w.scenarios.Files.Read(ctx, path)
	if err != nil {
		return OpenResult{}, err
	}
	if !utf8.Valid(source) {
		return OpenResult{}, domain.Problem("parse", "invalidUtf8", "$", "Scenario files must use UTF-8.")
	}
	validation := w.Inspect(ctx, string(source), format)
	for i := range validation.Diagnostics {
		validation.Diagnostics[i].FilePath = &path
	}
	return OpenResult{Path: path, Format: format, Source: string(source), Validation: validation}, nil
}
func (w *Workspace) SaveDraft(ctx context.Context, path, source string) (ValidationResult, error) {
	if w.scenarios == nil || w.scenarios.Writer == nil {
		return ValidationResult{}, domain.ErrUnavailable
	}
	doc, err := w.scenarios.Codec.Decode(ctx, []byte(source), "json")
	if err != nil {
		return ValidationResult{}, err
	}
	nodes, edges := 0, 0
	for _, flow := range doc.Workflows {
		nodes += len(flow.Nodes)
		edges += len(flow.Edges)
		if len(flow.Inputs) > 32 || len(flow.Outputs) > 32 {
			return ValidationResult{}, draftLimit()
		}
	}
	if len(doc.Workflows) > 32 || nodes > 256 || edges > 1024 {
		return ValidationResult{}, draftLimit()
	}
	value, err := admittedWire(doc)
	if err != nil {
		return ValidationResult{}, err
	}
	wire, err := jsonWire(value)
	if err != nil {
		return ValidationResult{}, err
	}
	var pretty bytes.Buffer
	if err = json.Indent(&pretty, wire, "", "  "); err != nil {
		return ValidationResult{}, err
	}
	// Re-admission applies the same source/depth/schema bounds to serialized data.
	if _, err = w.scenarios.Codec.Decode(ctx, pretty.Bytes(), "json"); err != nil {
		return ValidationResult{}, err
	}
	if err = w.scenarios.Writer.WriteJSON(ctx, path, pretty.Bytes()); err != nil {
		return ValidationResult{}, err
	}
	return w.Inspect(ctx, pretty.String(), "json"), nil
}
func draftLimit() error {
	return domain.Problem("resourceLimit", "draftResourceLimit", "/workflows", "Workflow, node, edge, or port limits exceeded.")
}
