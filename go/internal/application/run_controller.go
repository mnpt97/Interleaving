package application

import (
	"context"
	"encoding/json"
	"errors"
	"interleaving/internal/domain"
	"strings"
	"unicode/utf8"
)

func (w *Workspace) StartFile(ctx context.Context, path string, options RunOptions) (*RunHandle, error) {
	if w.runs == nil || w.scenarios == nil {
		return nil, domain.ErrUnavailable
	}
	source, format, err := w.scenarios.Files.Read(ctx, path)
	if err != nil {
		return nil, err
	}
	return w.Start(ctx, RunRequest{Source: source, Format: format, Options: options})
}
func (w *Workspace) CheckReportPath(ctx context.Context, input, output string) error {
	if w.runs == nil {
		return domain.ErrUnavailable
	}
	return w.runs.Reports.Check(ctx, input, output)
}
func (w *Workspace) SaveReport(ctx context.Context, input, output string, report RunReport) error {
	if w.runs == nil {
		return domain.ErrUnavailable
	}
	data, err := json.Marshal(report)
	if err != nil {
		return fail("reportWrite", "Report could not be serialized")
	}
	return w.runs.Reports.Write(ctx, input, output, data)
}
func (w *Workspace) Cancel(id string) bool {
	w.runMu.Lock()
	handle := w.active[id]
	w.runMu.Unlock()
	if handle == nil {
		return false
	}
	handle.Cancel()
	return true
}

// Start completes all admission and credential resolution before spawning HTTP.
// Runtime state is isolated per run; only safe public data reaches the handle.
func (w *Workspace) Start(ctx context.Context, request RunRequest) (*RunHandle, error) {
	if w.runs == nil {
		return nil, domain.ErrUnavailable
	}
	options := request.Options
	options.SensitiveFields = append([]string(nil), options.SensitiveFields...)
	if options.MaxRequestBytes == 0 && options.MaxResponseBytes == 0 && options.MaxPreviewBytes == 0 && options.MaxReportBytes == 0 && options.EventCapacity == 0 && len(options.SensitiveFields) == 0 {
		options = DefaultRunOptions()
	}
	if options.MaxRequestBytes < 1 || options.MaxRequestBytes > 1048576 || options.MaxResponseBytes < 1 || options.MaxResponseBytes > 1048576 || options.MaxPreviewBytes < 1 || options.MaxPreviewBytes > 65536 || options.MaxReportBytes < 131072 || options.MaxReportBytes > 16777216 || options.EventCapacity < 1 || options.EventCapacity > 1024 || len(options.SensitiveFields) > 64 {
		return nil, domain.Problem("validation", "runConfiguration", "$", "Invalid run resource limits")
	}
	for _, field := range options.SensitiveFields {
		if len(field) > 128 {
			return nil, domain.Problem("validation", "runConfiguration", "$", "Invalid sensitive field limit")
		}
	}
	program, err := w.Compile(ctx, request.Source, request.Format)
	if err != nil {
		return nil, err
	}
	doc := program.Document
	security := &security{secrets: map[string]bool{}, urls: w.runs.URLs, fields: options.SensitiveFields}
	credentials := map[string]string{}
	security.learnDocument(doc)
	security.conservative = len(program.RequiredSecrets) > 0
	for _, i := range program.Reachable {
		flow := program.Flows[i]
		definition := doc.Workflows[i]
		if len(flow.ResponseLastUses) > 0 || len(definition.Inputs) > 0 || len(definition.Outputs) > 0 {
			security.conservative = true
		}
		for _, n := range definition.Nodes {
			if n.Type == "setSession" || n.Type == "extract" {
				security.conservative = true
			}
		}
	}
	group := doc.Groups[0]
	var profile domain.ClientProfile
	for _, p := range doc.ClientProfiles {
		if p.ID == group.ClientProfileID {
			profile = p
		}
	}
	session, problem := w.runs.Sessions.New(profile, options)
	if problem != nil {
		return nil, domain.Problem("validation", "runConfiguration", "$", problem.Message)
	}
	prepared := false
	defer func() {
		if !prepared {
			session.Close()
		}
	}()
	runtime := &runner{workspace: w, program: program, session: session, security: security, credentials: credentials, options: options, start: w.clock.Now(), frames: []*frame{}}
	for _, i := range program.Reachable {
		for _, n := range doc.Workflows[i].Nodes {
			if n.Type != "http" && n.Type != "authentication" {
				continue
			}
			input, problem := runtime.httpInput(n.Config, nil)
			if problem != nil {
				return nil, domain.Problem("validation", "runConfiguration", "$", problem.Message)
			}
			wire, problem := session.Prepare(input)
			if problem != nil {
				return nil, domain.Problem("validation", "runConfiguration", "$", problem.Message)
			}
			security.url(wire.URL)
		}
	}
	byName := map[string]string{}
	total := 0
	if len(program.RequiredSecrets) > 4096 {
		return nil, domain.Problem("resourceLimit", "secretLimit", "/identities", "Credential references exceed limits")
	}
	for _, required := range program.RequiredSecrets {
		name := required.Reference.Name
		value, ok := byName[name]
		if !ok {
			var lookup error
			value, lookup = w.runs.Credentials.Lookup(ctx, name)
			if lookup != nil {
				code := "secretProviderUnavailable"
				if errors.Is(lookup, ErrMissingCredential) {
					code = "missingSecret"
				}
				if ctx.Err() != nil {
					return nil, ctx.Err()
				}
				return nil, domain.Problem("validation", code, "/identities", "Required environment credential unavailable. Environment reference: "+name+".")
			}
			if !utf8.ValidString(value) {
				return nil, domain.Problem("validation", "nonUnicodeSecret", "/identities", "Credential is not valid Unicode")
			}
			if value == "" {
				return nil, domain.Problem("validation", "emptySecret", "/identities", "Credential is empty")
			}
			total += len(value)
			if len(value) > 8192 || total > 1048576 {
				return nil, domain.Problem("resourceLimit", "secretLimit", "/identities", "Credential values exceed limits")
			}
			byName[name] = value
		}
		credentials[required.IdentityID+"\x00"+required.Field] = value
		security.remember(value)
	}
	snapshot := security.snapshot(doc, program.Targets)
	graph := graphProjection(program, security)
	encoded, _ := json.Marshal([]any{snapshot, graph, graph})
	if len(encoded)+69632 > options.MaxReportBytes {
		return nil, domain.Problem("resourceLimit", "runConfiguration", "$", "Snapshot and graph exceed report budget")
	}
	control := newControl(ctx)
	events := make(chan RunEvent, options.EventCapacity)
	handle := &RunHandle{ID: w.ids.NewID(), Events: events, control: control, done: make(chan struct{})}
	runtime.handle = handle
	runtime.events = events
	runtime.report = RunReport{ReportVersion: 3, RunID: handle.ID, Snapshot: snapshot, Graph: graph, Events: []RunEvent{}, Cleanup: CleanupOutcome{Status: "notConfigured"}}
	runtime.reportBytes = len(encoded) + 1024
	w.runMu.Lock()
	w.active[handle.ID] = handle
	w.runMu.Unlock()
	prepared = true
	stopParent := context.AfterFunc(ctx, handle.Cancel)
	w.logger.Log(ctx, Info, "run started", Field{Name: "runId", Value: handle.ID})
	go func() {
		defer close(handle.done)
		defer close(events)
		defer session.Close()
		defer control.shutdown()
		defer control.cancel()
		defer stopParent()
		runtime.run()
		handle.report = runtime.report
		w.runMu.Lock()
		delete(w.active, handle.ID)
		w.runMu.Unlock()
		if handle.report.Error != nil {
			w.logger.Log(context.WithoutCancel(ctx), Error, "run failed", Field{Name: "runId", Value: handle.ID}, Field{Name: "kind", Value: handle.report.Error.Kind})
		}
		w.logger.Log(context.WithoutCancel(ctx), Info, "run finished", Field{Name: "runId", Value: handle.ID}, Field{Name: "status", Value: handle.report.Status})
	}()
	return handle, nil
}
func authKey(w, n int) string {
	return strings.Repeat("authentication", 11) + ":" + integerText(w) + ":" + integerText(n)
}
