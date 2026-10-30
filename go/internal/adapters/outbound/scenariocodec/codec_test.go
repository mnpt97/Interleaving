package scenariocodec_test

import (
	"context"
	"errors"
	"strings"
	"testing"

	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/domain"
)

func TestAdmissionBudgetsApplyBeforeSchemaValidation(t *testing.T) {
	for _, test := range []struct {
		name, source, format, code, path string
		limits                           scenariocodec.Limits
	}{
		{"bytes", "     ", "json", "fileSizeLimit", "$", scenariocodec.Limits{Bytes: 4, Depth: 64, Collection: 100, Values: 100}},
		{"json-depth", "[[[0]]]", "json", "resourceLimit", "/0/0/0", scenariocodec.Limits{Bytes: 1000, Depth: 2, Collection: 100, Values: 100}},
		{"yaml-depth", "[[[0]]]", "yaml", "resourceLimit", "/0/0/0", scenariocodec.Limits{Bytes: 1000, Depth: 2, Collection: 100, Values: 100}},
		{"values", "[1,2,3]", "json", "resourceLimit", "/2", scenariocodec.Limits{Bytes: 1000, Depth: 64, Collection: 100, Values: 3}},
		{"yaml-values", "[1,2,3]", "yaml", "resourceLimit", "/2", scenariocodec.Limits{Bytes: 1000, Depth: 64, Collection: 100, Values: 3}},
	} {
		t.Run(test.name, func(t *testing.T) {
			codec, e := scenariocodec.NewWithLimits(test.limits)
			if e != nil {
				t.Fatal(e)
			}
			_, err := codec.Decode(context.Background(), []byte(test.source), test.format)
			var d domain.Diagnostic
			if !errors.As(err, &d) || d.Code != test.code || d.FieldPath != test.path {
				t.Fatalf("%+v", err)
			}
		})
	}
}
func TestErrorsAreSafeAndDocumentLocationsAreSeparate(t *testing.T) {
	codec, e := scenariocodec.New()
	if e != nil {
		t.Fatal(e)
	}
	for _, format := range []string{"json", "yaml"} {
		source := "{\n\"x\": 1,\n\"x\": \"SECRET_SENTINEL\"}"
		if format == "yaml" {
			source = "x: 1\nx: SECRET_SENTINEL\n"
		}
		_, err := codec.Decode(context.Background(), []byte(source), format)
		var d domain.Diagnostic
		if !errors.As(err, &d) || d.Code != "duplicateKey" || d.Line == nil || *d.Line < 2 || d.Column == nil || d.ExpressionLine != nil || strings.Contains(err.Error(), "SECRET_SENTINEL") {
			t.Fatalf("%+v", err)
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := codec.Decode(ctx, []byte("{}"), "json"); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

// Seeded fuzzing runs in normal go test; longer fuzz sessions are optional. Keep
// failures safe for arbitrary bytes, malformed trees, and unusual Unicode input.
func FuzzDecodeNeverPanics(f *testing.F) {
	for _, s := range []string{"{}", "[]", "null", "{\"schemaVersion\":2}", "{\"x\":\"\\ud800\"}", "x: &a [*a]", "[[]]"} {
		f.Add(s)
	}
	codec, e := scenariocodec.New()
	if e != nil {
		f.Fatal(e)
	}
	f.Fuzz(func(t *testing.T, s string) {
		if len(s) > 10000 {
			t.Skip()
		}
		for _, format := range []string{"json", "yaml"} {
			_, _ = codec.Decode(context.Background(), []byte(s), format)
		}
	})
}
