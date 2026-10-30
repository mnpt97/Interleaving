// Command interleaving-go is the desktop composition root. It has no Rust subprocess fallback.
package main

import (
	"context"
	"embed"
	"os"

	"github.com/wailsapp/wails/v3/pkg/application"

	adapter "interleaving/internal/adapters/inbound/wails"
	"interleaving/internal/adapters/outbound/charmlog"
	core "interleaving/internal/application"
	"interleaving/internal/bootstrap"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	logger := charmlog.New(os.Stderr)
	service := adapter.New(bootstrap.Workspace(logger))
	app := application.New(application.Options{
		Logger: logger.Slog(),
		Name:   "Interleaving Go", Description: "Interleaving desktop",
		Services: []application.Service{application.NewService(service)},
		Assets:   application.AssetOptions{Handler: application.AssetFileServerFS(assets)},
		Mac:      application.MacOptions{ApplicationShouldTerminateAfterLastWindowClosed: true},
	})
	window := app.Window.NewWithOptions(application.WebviewWindowOptions{Title: "Interleaving Go", Name: "interleaving-go", Width: 1280, Height: 820, MinWidth: 920, MinHeight: 640, URL: "/"})
	adapter.Bind(service, app, window)
	if err := app.Run(); err != nil {
		logger.Log(context.Background(), core.Error, "Desktop initialization failed")
		os.Exit(3)
	}
}
