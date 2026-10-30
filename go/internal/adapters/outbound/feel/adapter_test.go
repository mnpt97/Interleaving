package feel_test

import (
	"encoding/json"
	"testing"

	"interleaving/internal/adapters/outbound/feel"
)

func TestParserAdapterPreservesInputBoundariesAndCancellation(t *testing.T) {
	a := feel.Adapter{}
	p, e := a.CompilePredicate(`inputs.minimum >= 1 and inputs.label = "ready"`)
	if e != nil {
		t.Fatal(e)
	}
	value, err := p.Evaluate(200, nil, map[string]any{"minimum": json.Number("1"), "label": "ready"}, nil)
	if err != nil || !value {
		t.Fatalf("%v %v", value, err)
	}
	calls := 0
	_, err = p.Evaluate(200, nil, nil, func() bool { calls++; return calls == 2 })
	if err == nil || err.Error() != "cancelled" {
		t.Fatal(err)
	}
	for _, s := range []string{`true /* comment */`, `true // comment
`, `now()`, `1;true`, `(response.status) = 200`, `response.status = (200)`} {
		if _, e := a.CompilePredicate(s); e == nil {
			t.Fatalf("admitted unsupported syntax: %q", s)
		}
	}
	p, e = a.CompilePredicate("response.status > - 1")
	if e != nil {
		t.Fatal(e)
	}
	if value, err = p.Evaluate(200, nil, nil, nil); err != nil || !value {
		t.Fatalf("spaced negative integer: %v %v", value, err)
	}
}
func FuzzCompileNeverPanics(f *testing.F) {
	for _, s := range []string{`true`, `response.status = 200`, `inputs.a = inputs.b and inputs.c`, `not(true)`, `"\\ud800"`, `response.body.x`, `(true)`, `response.body.`} {
		f.Add(s)
	}
	a := feel.Adapter{}
	f.Fuzz(func(t *testing.T, s string) {
		if len(s) > 5000 {
			t.Skip()
		}
		_, _ = a.CompilePredicate(s)
		_, _ = a.CompileExtraction(s)
	})
}
