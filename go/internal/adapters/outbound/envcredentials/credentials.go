// Package envcredentials resolves explicit references without reading .env files.
package envcredentials

import (
	"context"
	"interleaving/internal/application"
	"os"
)

type Adapter struct{}

func (Adapter) Lookup(ctx context.Context, name string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	value, ok := os.LookupEnv(name)
	if !ok {
		return "", application.ErrMissingCredential
	}
	return value, nil
}
