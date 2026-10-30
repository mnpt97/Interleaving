package application_test

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"testing"

	"interleaving/internal/adapters/outbound/feel"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/adapters/outbound/targeturls"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

type memoryCodec struct {
	doc   *domain.Document
	calls int
}

func (c *memoryCodec) Decode(context.Context, []byte, string) (*domain.Document, error) {
	c.calls++
	return c.doc, nil
}

type memoryFiles struct{ calls int }

func (f *memoryFiles) Read(context.Context, string) ([]byte, string, error) {
	f.calls++
	return []byte("adapter-owned representation"), "json", nil
}
func testDocument(t *testing.T) *domain.Document {
	t.Helper()
	source, e := os.ReadFile("../../../scenarios/examples/hello-api.json")
	if e != nil {
		t.Fatal(e)
	}
	codec, e := scenariocodec.New()
	if e != nil {
		t.Fatal(e)
	}
	doc, e := codec.Decode(context.Background(), source, "json")
	if e != nil {
		t.Fatal(e)
	}
	return doc
}
func TestValidationUsesReplaceablePortsAndPreservesPlanSnapshot(t *testing.T) {
	doc := testDocument(t)
	codec := &memoryCodec{doc: doc}
	files := &memoryFiles{}
	app := application.NewWorkspace(fixedClock{}, fixedIDs{}, &recordingLog{}, application.WithScenarios(application.ScenarioServices{Codec: codec, Files: files, Expressions: feel.Adapter{}, Targets: targeturls.Adapter{}}))
	if err := app.ValidateFile(context.Background(), "virtual-scenario"); err != nil {
		t.Fatal(err)
	}
	if files.calls != 1 || codec.calls != 1 || !app.Capabilities().Validation || app.Capabilities().Execution {
		t.Fatal("ports or capabilities incorrect")
	}
	plan, err := app.Compile(context.Background(), nil, "json")
	if err != nil {
		t.Fatal(err)
	}
	doc.Workflows[0].Nodes[1].Config.Path = "/changed"
	if plan.Document.Workflows[0].Nodes[1].Config.Path != "/health" {
		t.Fatal("caller mutation changed compiled plan")
	}
	if len(plan.Flows) != 1 || len(plan.Flows[0].Order) != 3 || len(plan.Reachable) != 1 {
		t.Fatal("missing graph plan")
	}
	if _, err := app.Start(context.Background(), application.RunRequest{}); !errors.Is(err, domain.ErrUnavailable) {
		t.Fatal("execution must remain unavailable")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	before := codec.calls
	if _, err = app.Compile(ctx, nil, "json"); !errors.Is(err, context.Canceled) || codec.calls != before {
		t.Fatal("cancellation must precede adapter calls")
	}
}
func TestShowcaseCompilationInfersPairsAndRetainsTypedDependencies(t *testing.T) {
	source, err := os.ReadFile("../../../scenarios/examples/feature-showcase.interl.json")
	if err != nil {
		t.Fatal(err)
	}
	codec, err := scenariocodec.New()
	if err != nil {
		t.Fatal(err)
	}
	app := application.NewWorkspace(fixedClock{}, fixedIDs{}, &recordingLog{}, application.WithScenarios(application.ScenarioServices{Codec: codec, Expressions: feel.Adapter{}, Targets: targeturls.Adapter{}}))
	program, err := app.Compile(context.Background(), source, "json")
	if err != nil {
		t.Fatal(err)
	}
	if program.Flows[0].Pairs[4] != 7 || program.Flows[0].Predicates[4] == nil || !program.Flows[0].SensitiveHTTP["health"] || len(program.Flows[2].Extractions["extract-health"]) != 3 || len(program.RequiredSecrets) != 5 || len(program.Reachable) != 4 {
		t.Fatalf("incomplete showcase compilation: %+v", program)
	}
	if !program.Flows[0].Dominators["route"]["inspect"] || program.Flows[0].Dominators["joined"]["accepted"] {
		t.Fatal("branch dominance incorrect")
	}
	values := program.Flows[2].ExtractionValues["extract-health"]
	if len(values) != 3 {
		t.Fatal("missing external extraction handles")
	}
	body := map[string]any{"value": map[string]any{"status": true, "attempts": json.Number("3"), "label": "ready"}}
	if actual, err := values["ready"].Evaluate(body, nil); err != nil || actual != true {
		t.Fatalf("external extraction: %v %v", actual, err)
	}
	token := program.Flows[1].AuthTokens["bearer-login"]
	if token == nil {
		t.Fatal("missing external authentication token expression")
	}
	if actual, err := token.Evaluate(map[string]any{"accessToken": "fake-test-token"}, nil); err != nil || actual != "fake-test-token" {
		t.Fatalf("external token extraction: %v %v", actual, err)
	}
}
