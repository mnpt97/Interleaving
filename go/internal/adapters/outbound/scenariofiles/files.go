// Package scenariofiles implements bounded local reads without leaking OS errors.
package scenariofiles

import (
	"context"
	"io"
	"os"
	"path/filepath"

	"interleaving/internal/domain"
)

type Adapter struct{}

func (Adapter) Read(ctx context.Context, path string) ([]byte, string, error) {
	if err := ctx.Err(); err != nil {
		return nil, "", err
	}
	format := ""
	switch filepath.Ext(path) {
	case ".json":
		format = "json"
	case ".yaml", ".yml":
		format = "yaml"
	default:
		return nil, "", domain.Problem("format", "unsupportedExtension", "", "Use a JSON or YAML scenario file.")
	}
	info, err := os.Stat(path)
	if err != nil {
		return nil, "", domain.Problem("io", "readFile", "", "Cannot access scenario file; check its path and permissions.")
	}
	if !info.Mode().IsRegular() {
		return nil, "", domain.Problem("io", "notFile", "", "Select a regular scenario file.")
	}
	const maxBytes = 1024 * 1024
	if info.Size() > maxBytes {
		return nil, "", domain.Problem("resourceLimit", "fileSizeLimit", "", "Scenario file exceeds the configured byte limit.")
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, "", domain.Problem("io", "readFile", "", "Cannot open scenario file.")
	}
	defer file.Close()
	source, err := io.ReadAll(io.LimitReader(file, maxBytes+1))
	if err != nil {
		return nil, "", domain.Problem("io", "readFile", "", "Cannot read scenario file.")
	}
	if len(source) > maxBytes {
		return nil, "", domain.Problem("resourceLimit", "fileSizeLimit", "", "Scenario file exceeds the configured byte limit.")
	}
	if err := ctx.Err(); err != nil {
		return nil, "", err
	}
	return source, format, nil
}
