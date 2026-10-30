// Package reportfiles owns input-alias checks and atomic private report writes.
package reportfiles

import (
	"context"
	"interleaving/internal/application"
	"os"
	"path/filepath"
)

type Adapter struct{}

func problem(message string) error {
	return &application.Failure{Kind: "reportWrite", Message: message}
}
func (Adapter) Check(ctx context.Context, input, output string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	a, err := filepath.Abs(input)
	if err != nil {
		return problem("Report path is invalid")
	}
	b, err := filepath.Abs(output)
	if err != nil {
		return problem("Report path is invalid")
	}
	if a == b {
		return problem("Report output must differ from the scenario input")
	}
	ai, ae := os.Stat(input)
	bi, be := os.Stat(output)
	if ae == nil && be == nil && os.SameFile(ai, bi) {
		return problem("Report output must differ from the scenario input")
	}
	if be == nil && !bi.Mode().IsRegular() {
		return problem("Report output must be a regular file")
	}
	return nil
}
func (a Adapter) Write(ctx context.Context, input, output string, data []byte) error {
	if err := a.Check(ctx, input, output); err != nil {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(output), ".interl-report-*")
	if err != nil {
		return problem("Report could not be written; check output permissions")
	}
	name := f.Name()
	defer os.Remove(name)
	if _, err = f.Write(data); err != nil {
		f.Close()
		return problem("Report could not be written")
	}
	if err = f.Close(); err != nil {
		return problem("Report could not be closed")
	}
	if err = a.Check(ctx, input, output); err != nil {
		return err
	}
	if err = os.Rename(name, output); err != nil {
		return problem("Report could not be published")
	}
	return nil
}
