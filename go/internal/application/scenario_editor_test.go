package application_test

import (
	"context"
	"encoding/json"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/bootstrap"
	"io"
	"os"
	"path/filepath"
	"testing"
)

func TestEditorInspectsBothFormatsAndRetainsInvalidDrafts(t *testing.T) {
	app := bootstrap.Workspace(charmlog.New(io.Discard))
	for _, format := range []string{"json", "yaml"} {
		path := "../../../scenarios/examples/feature-showcase.interl." + format
		opened, err := app.OpenScenario(context.Background(), path)
		if err != nil {
			t.Fatal(err)
		}
		if !opened.Validation.Valid || opened.Validation.Document == nil || opened.Source == "" || opened.Format != format {
			t.Fatalf("bad open: %+v", opened.Validation.Diagnostics)
		}
	}
	doc := basicDoc(t)
	doc["workflows"].([]any)[0].(map[string]any)["nodes"].([]any)[1].(map[string]any)["config"].(map[string]any)["path"] = ""
	source := string(encoded(t, doc))
	validation := app.Inspect(context.Background(), source, "json")
	if validation.Valid || validation.Document == nil || len(validation.Diagnostics) == 0 {
		t.Fatal("semantic error discarded editable document")
	}
	path := filepath.Join(t.TempDir(), "draft.interl.json")
	saved, err := app.SaveDraft(context.Background(), path, source)
	if err != nil {
		t.Fatal(err)
	}
	if saved.Valid || saved.Document == nil {
		t.Fatal("unfinished HTTP node must remain editable")
	}
	opened, err := app.OpenScenario(context.Background(), path)
	if err != nil {
		t.Fatal(err)
	}
	if opened.Validation.Valid || opened.Validation.Document == nil {
		t.Fatal("saved draft cannot reopen")
	}
	before, _ := os.ReadFile(path)
	if _, err = app.SaveDraft(context.Background(), path, "{"); err == nil {
		t.Fatal("malformed source overwritten")
	}
	after, _ := os.ReadFile(path)
	if string(before) != string(after) {
		t.Fatal("invalid save modified file")
	}
	if _, err = app.SaveDraft(context.Background(), filepath.Join(t.TempDir(), "draft.yaml"), source); err == nil {
		t.Fatal("JSON overwrote YAML destination")
	}
	var roundtrip any
	if err = json.Unmarshal(after, &roundtrip); err != nil {
		t.Fatal(err)
	}
}

func TestEditorReadWriteFailuresAreSafeAndStructuralFailuresHaveNoDocument(t *testing.T) {
	app := bootstrap.Workspace(charmlog.New(io.Discard))
	if result := app.Inspect(context.Background(), `{"SECRET_SENTINEL":`, "json"); result.Document != nil || result.Valid || len(result.Diagnostics) == 0 {
		t.Fatal("invalid structural result")
	}
	if _, err := app.OpenScenario(context.Background(), filepath.Join(t.TempDir(), "missing.json")); err == nil {
		t.Fatal("missing file accepted")
	}
	doc := basicDoc(t)
	if _, err := app.SaveDraft(context.Background(), filepath.Join(t.TempDir(), "missing", "file.json"), string(encoded(t, doc))); err == nil {
		t.Fatal("missing output directory accepted")
	}
}
