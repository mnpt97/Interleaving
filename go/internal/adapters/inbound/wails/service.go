// Package wails is a driving adapter; native runtime types never enter the core.
package wails

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"strings"
	"sync/atomic"

	native "github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"

	"interleaving/internal/adapters/inbound/desktop"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

// Service exposes native dialogs, scenario operations and the independent probe.
type Service struct {
	app     *native.App
	window  *native.WebviewWindow
	core    application.WorkspacePort
	ready   atomic.Int32
	closing atomic.Bool
	desktop *desktop.Session
}

func New(core application.WorkspacePort) *Service {
	return &Service{core: core, desktop: desktop.New(core)}
}

// Bind runs in the desktop composition root after the native window exists.
func Bind(s *Service, app *native.App, window *native.WebviewWindow) {
	s.app = app
	s.window = window
	app.OnShutdown(s.desktop.Close)
	app.HandleStream("interleaving:run", func(c *native.StreamConn) {
		defer c.Close()
		raw, err := c.Receive()
		if err != nil {
			return
		}
		var request struct {
			ProtocolVersion int    `json:"protocolVersion"`
			Source          string `json:"source"`
			Format          string `json:"format"`
		}
		decode := json.NewDecoder(bytes.NewReader(raw))
		decode.DisallowUnknownFields()
		invalid := len(raw) > 3*1024*1024
		if !invalid {
			invalid = decode.Decode(&request) != nil
			if !invalid {
				invalid = decode.Decode(new(any)) != io.EOF
			}
		}
		if invalid || request.ProtocolVersion != 3 || (request.Format != "json" && request.Format != "yaml") {
			_ = c.SendJSON(desktop.Message{ProtocolVersion: 3, Kind: "rejected", Diagnostics: application.Diagnostics(domain.Problem("validation", "executionProtocol", "$", "Execution protocol is incompatible. Restart the desktop app."))})
			return
		}
		s.desktop.Serve(c.Context(), request.Source, request.Format, func(message desktop.Message) error { return c.SendJSON(message) })
	})
	window.RegisterHook(events.Common.WindowClosing, func(event *native.WindowEvent) {
		if s.ready.Load() == 0 || s.closing.Load() {
			return
		}
		event.Cancel()
		s.window.EmitEvent("interleaving:close-requested")
	})
}
func (s *Service) Ready(enabled bool) {
	if enabled {
		s.ready.Add(1)
		return
	}
	for {
		count := s.ready.Load()
		if count == 0 || s.ready.CompareAndSwap(count, count-1) {
			return
		}
	}
}
func (s *Service) Capabilities() domain.Capabilities { return s.core.Capabilities() }

// Probe returns a bounded synthetic catalogue. It cannot invoke the scenario engine or HTTP.
func (s *Service) Probe(ctx context.Context, count int) ([]application.ProbeEvent, error) {
	stream, err := s.core.Probe(ctx, count)
	if err != nil {
		return nil, err
	}
	result := make([]application.ProbeEvent, 0, count)
	for event := range stream {
		result = append(result, event)
	}
	return result, nil
}

// StreamProbe uses a distinct topic and never masquerades as protocol-v3 execution.
func (s *Service) StreamProbe(ctx context.Context, count int) error {
	stream, err := s.core.Probe(ctx, count)
	if err != nil {
		return err
	}
	for event := range stream {
		if err := ctx.Err(); err != nil {
			return err
		}
		s.window.EmitEvent("interleaving:probe", event)
	}
	return nil
}
func (s *Service) OpenDialog() (string, error) {
	return s.app.Dialog.OpenFile().SetTitle("Open scenario").AddFilter("Scenarios", "*.json;*.yaml;*.yml").AttachToWindow(s.window).PromptForSingleSelection()
}
func (s *Service) SaveDialog(filename string) (string, error) {
	filename = strings.Map(func(r rune) rune {
		if r == '/' || r == '\\' || r < 32 {
			return '-'
		}
		return r
	}, filename)
	if len(filename) > 160 {
		filename = "scenario.interl.json"
	}
	return s.app.Dialog.SaveFile().SetMessage("Save scenario").SetFilename(filename).AddFilter("JSON", "*.json").AttachToWindow(s.window).PromptForSingleSelection()
}
func (s *Service) SaveReportDialog() (string, error) {
	return s.app.Dialog.SaveFile().SetMessage("Export report").SetFilename("interl-results.json").AddFilter("JSON", "*.json").AttachToWindow(s.window).PromptForSingleSelection()
}
func (s *Service) Close() { s.closing.Store(true); s.desktop.Close(); s.window.Close() }
func (s *Service) ApplyTheme(theme string) error {
	return applyNativeTheme(s.window, theme)
}

func (s *Service) ValidateSource(ctx context.Context, source, format string) application.ValidationResult {
	return s.core.Inspect(ctx, source, format)
}
func (s *Service) ReadScenario(ctx context.Context, path string) desktop.Result {
	value, err := s.core.OpenScenario(ctx, path)
	if err != nil {
		return desktop.Failed(err)
	}
	return desktop.Success(value)
}
func (s *Service) SaveScenarioJSON(ctx context.Context, path, source string) desktop.Result {
	value, err := s.core.SaveDraft(ctx, path, source)
	if err != nil {
		return desktop.Failed(err)
	}
	return desktop.Success(value)
}
func (s *Service) CancelScenario(id string) bool { return s.desktop.Cancel(id) }
func (s *Service) SaveRunReport(ctx context.Context, path, id string) desktop.Result {
	if !strings.HasSuffix(strings.ToLower(path), ".json") {
		return desktop.Failed(domain.Problem("format", "unsupportedExtension", "$", "Choose a .json report file."))
	}
	return s.desktop.SaveReport(ctx, path, id)
}
