package application_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"interleaving/internal/adapters/outbound/system"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

type fixedClock struct{}

func (fixedClock) Now() time.Time { return time.Unix(0, 0).UTC() }

type fixedIDs struct{}

func (fixedIDs) NewID() string { return "test-run" }

type recordingLog struct{ calls int }

func (r *recordingLog) Log(context.Context, application.Level, string, ...application.Field) {
	r.calls++
}

func TestProbeAdapterReplacement(t *testing.T) {
	for _, app := range []*application.Workspace{application.NewWorkspace(fixedClock{}, fixedIDs{}, &recordingLog{}), application.NewWorkspace(system.Clock{}, system.IDs{}, &recordingLog{})} {
		events, err := app.Probe(context.Background(), 16)
		if err != nil {
			t.Fatal(err)
		}
		n := 0
		id := ""
		for event := range events {
			n++
			if event.Sequence != n || !event.Synthetic || event.ID == "" {
				t.Fatal("invalid probe event")
			}
			if id != "" && id != event.ID {
				t.Fatal("identity changed")
			}
			id = event.ID
		}
		if n != 16 {
			t.Fatal(n)
		}
		if _, err := app.Probe(context.Background(), 17); err == nil {
			t.Fatal("unbounded probe")
		}
		if !errors.Is(app.Validate(context.Background(), "{}", "json"), domain.ErrUnavailable) {
			t.Fatal("false validation")
		}
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		if _, err := app.Probe(ctx, 1); !errors.Is(err, context.Canceled) {
			t.Fatal(err)
		}
	}
}

func TestLoggingPortIsInjectedWithoutChangingProbeEvents(t *testing.T) {
	logger := &recordingLog{}
	app := application.NewWorkspace(fixedClock{}, fixedIDs{}, logger)
	events, err := app.Probe(context.Background(), 1)
	if err != nil {
		t.Fatal(err)
	}
	event := <-events
	if event.ID != "test-run" || event.Sequence != 1 || logger.calls != 1 {
		t.Fatal("logging injection changed probe or was not called")
	}
}
