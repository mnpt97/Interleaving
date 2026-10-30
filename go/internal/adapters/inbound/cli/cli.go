// Package cli maps use cases to headless command output and exit codes.
package cli

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"

	"interleaving/internal/application"
	"interleaving/internal/domain"
)

func Run(ctx context.Context, args []string, out, stderr io.Writer, app application.WorkspacePort) int {
	if len(args) == 0 || args[0] == "--help" || args[0] == "help" {
		fmt.Fprintln(out, "interl-go status | probe | validate <scenario> | run <scenario> [--output <report>]")
		return 0
	}
	switch args[0] {
	case "status":
		if err := json.NewEncoder(out).Encode(app.Capabilities()); err != nil {
			return 3
		}
		return 0
	case "probe":
		events, err := app.Probe(ctx, 3)
		if err != nil {
			fmt.Fprintln(stderr, err)
			return 3
		}
		for event := range events {
			if err := json.NewEncoder(out).Encode(event); err != nil {
				return 3
			}
		}
		return 0
	case "validate":
		if len(args) != 2 {
			fmt.Fprintln(stderr, "Usage: interl-go validate <scenario>")
			return 2
		}
		if err := app.ValidateFile(ctx, args[1]); err != nil {
			var many domain.Diagnostics
			var one domain.Diagnostic
			if errors.As(err, &many) {
				for _, d := range many {
					writeDiagnostic(stderr, d)
				}
			} else if errors.As(err, &one) {
				writeDiagnostic(stderr, one)
			} else if errors.Is(err, domain.ErrUnavailable) {
				fmt.Fprintln(stderr, "unavailable:", err)
			} else {
				fmt.Fprintln(stderr, "Validation cancelled or unavailable.")
			}
			return 2
		}
		if _, err := fmt.Fprintln(out, "Valid scenario:", args[1]); err != nil {
			return 3
		}
		return 0
	case "run":
		return run(ctx, args[1:], out, stderr, app)
	default:
		fmt.Fprintln(stderr, "Unknown command; use --help")
		return 2
	}
}

func writeDiagnostic(out io.Writer, d domain.Diagnostic) {
	if d.FilePath != nil {
		fmt.Fprint(out, *d.FilePath)
	}
	if d.Line != nil {
		column := 1
		if d.Column != nil {
			column = *d.Column
		}
		fmt.Fprintf(out, ":%d:%d", *d.Line, column)
	}
	category := d.Category
	if category != "" {
		category = strings.ToUpper(category[:1]) + category[1:]
	}
	fmt.Fprintf(out, " [%s/%s] %s: %s", category, d.Code, d.FieldPath, d.Message)
	if d.ExpressionLine != nil {
		column := 1
		if d.ExpressionColumn != nil {
			column = *d.ExpressionColumn
		}
		fmt.Fprintf(out, " (expression %d:%d)", *d.ExpressionLine, column)
	}
	fmt.Fprintln(out)
}
