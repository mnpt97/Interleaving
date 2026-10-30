package scenariofiles

import (
	"context"
	"interleaving/internal/domain"
	"os"
	"path/filepath"
)

// WriteJSON atomically saves a private draft without requiring a runnable graph.
func (Adapter) WriteJSON(ctx context.Context, path string, source []byte) error {
	if filepath.Ext(path) != ".json" {
		return domain.Problem("format", "unsupportedExtension", "$", "Save as a .json file; YAML files are never overwritten by JSON export.")
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	problem := func() error {
		return domain.Problem("io", "writeFile", "$", "Cannot save JSON; check the destination and permissions.")
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".interl-draft-*")
	if err != nil {
		return problem()
	}
	defer os.Remove(f.Name())
	if _, err = f.Write(source); err != nil {
		f.Close()
		return problem()
	}
	if err = f.Close(); err != nil {
		return problem()
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if err = os.Rename(f.Name(), path); err != nil {
		return problem()
	}
	return nil
}
