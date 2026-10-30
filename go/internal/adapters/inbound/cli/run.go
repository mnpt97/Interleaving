package cli

import (
	"context"
	"errors"
	"fmt"
	"interleaving/internal/application"
	"interleaving/internal/domain"
	"io"
	"strings"
)

// Command stdout is the existing event/summary protocol, not diagnostic logging.
// Execution diagnostics are emitted by the core through its injected logger port.
func run(ctx context.Context, args []string, out, stderr io.Writer, app application.WorkspacePort) int {
	if len(args) != 1 && (len(args) != 3 || args[1] != "--output") {
		fmt.Fprintln(stderr, "Usage: interl-go run <scenario> [--output <report>]")
		return 2
	}
	input, output := args[0], ""
	if len(args) == 3 {
		output = args[2]
		if err := app.CheckReportPath(ctx, input, output); err != nil {
			fmt.Fprintln(stderr, "Report output must differ from input and identify a writable regular file")
			return 2
		}
	}
	handle, err := app.StartFile(ctx, input, application.DefaultRunOptions())
	if err != nil {
		var many domain.Diagnostics
		var one domain.Diagnostic
		if errors.As(err, &many) {
			for _, d := range many {
				writeDiagnostic(stderr, d)
			}
		} else if errors.As(err, &one) {
			writeDiagnostic(stderr, one)
		} else if errors.Is(err, domain.ErrUnavailable) {
			fmt.Fprintln(stderr, "unavailable: Go execution services are not configured")
		} else {
			fmt.Fprintln(stderr, "Run preparation failed")
		}
		return 2
	}
	defer handle.Close()
	writeFailed := false
	for event := range handle.Events {
		if !writeFailed {
			if _, err := fmt.Fprintf(out, "%d %s\n", event.Sequence, event.Type); err != nil {
				writeFailed = true
				handle.Close()
			}
		}
	}
	report, err := handle.Wait(context.WithoutCancel(ctx))
	if err != nil {
		return 3
	}
	if output != "" {
		if err = app.SaveReport(context.WithoutCancel(ctx), input, output, report); err != nil {
			fmt.Fprintln(stderr, "Report could not be written; check output path and permissions")
			return 3
		}
	}
	fmt.Fprintf(out, "Run %s: %d requests, %d responses\n", strings.ToUpper(report.Status[:1])+report.Status[1:], report.Summary.Requests, report.Summary.Responses)
	if writeFailed {
		return 3
	}
	switch report.Status {
	case "passed":
		return 0
	case "cancelled":
		return 130
	default:
		return 3
	}
}
