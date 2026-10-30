package scenariocodec_test

import (
	"context"
	"encoding/json"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"strings"
	"testing"
)

func TestResponseJSONBudgetsAndPrivateDiagnostics(t *testing.T) {
	for _, test := range []struct{ name, source, kind string }{
		{"null", "null", ""}, {"safe-number", `9007199254740993`, ""},
		{"duplicate", `{"private-key":1,"private-key":2}`, "extractionDuplicateKey"},
		{"escaped-duplicate", `{"private-key":1,"private\u002dkey":2}`, "extractionDuplicateKey"},
		{"trailing", `{} {}`, "extractionInvalidJson"},
		{"surrogate", `"\ud800"`, "extractionInvalidJson"},
		{"depth", strings.Repeat("[", 65) + "0" + strings.Repeat("]", 65), "extractionJsonDepthLimit"},
		{"values", "[" + strings.Repeat("0,", 99999) + "0]", "extractionJsonValueLimit"},
		{"size", strings.Repeat(" ", 1048577), "extractionResponseLimit"},
	} {
		t.Run(test.name, func(t *testing.T) {
			value, err := (scenariocodec.ResponseJSON{}).Parse(context.Background(), []byte(test.source))
			if test.kind == "" {
				if err != nil {
					t.Fatal(err)
				}
				if test.name == "safe-number" && value != json.Number("9007199254740993") {
					t.Fatal("number rounded")
				}
				return
			}
			if err == nil || err.Kind != test.kind || strings.Contains(err.Message, "private-key") {
				t.Fatalf("unsafe/wrong error: %+v", err)
			}
		})
	}
}
