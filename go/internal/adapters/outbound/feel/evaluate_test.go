package feel

import (
	"encoding/json"
	"strings"
	"sync"
	"testing"

	library "github.com/pbinitiative/feel"
)

func TestEvaluationMatchesExternalInterpreter(t *testing.T) {
	for _, test := range []struct {
		expression string
		body       map[string]any
	}{
		{`response.body.flag = true`, map[string]any{"flag": nil}},
		{`response.body.left < response.body.right`, map[string]any{"left": false, "right": true}},
		{`response.body.flag != "true"`, map[string]any{"flag": true}},
		{`(response.body.count) and true`, map[string]any{"count": int64(1)}},
		{`(response.body.label) and true`, map[string]any{"label": "false"}},
		{`(response.body.flag) and true`, map[string]any{"flag": nil}},
		{`response.body.n = 1`, map[string]any{"n": 1.0}},
	} {
		t.Run(test.expression, func(t *testing.T) {
			native, err := library.EvalStringWithScope(test.expression, library.Scope{"response": map[string]any{"body": test.body}})
			if err != nil {
				t.Fatal(err)
			}
			compiled, e := (Adapter{}).CompilePredicate(test.expression)
			if e != nil {
				t.Fatal(e)
			}
			actual, err := compiled.Evaluate(200, test.body, nil, nil)
			if err != nil || native != actual {
				t.Fatalf("adapter %v/%v; external %v", actual, err, native)
			}
		})
	}
}

// A Node spy proves the adapter calls the external Node.Eval once, including a
// cancellation that arrives inside that call. It has no expression operations.
type countedNode struct {
	calls  *int
	cancel *bool
}

func (n countedNode) Eval(*library.Interpreter) (any, error) {
	*n.calls++
	if n.cancel != nil {
		*n.cancel = true
	}
	return true, nil
}
func (countedNode) Repr() string                 { return "test" }
func (countedNode) TextRange() library.TextRange { return library.TextRange{} }
func TestEvaluationDelegatesAndDiscardsCancelledResults(t *testing.T) {
	calls := 0
	p := &compiled{root: countedNode{calls: &calls}}
	if value, err := p.Evaluate(200, nil, nil, nil); err != nil || !value || calls != 1 {
		t.Fatalf("%v %v calls=%d", value, err, calls)
	}
	stop := false
	p.root = countedNode{calls: &calls, cancel: &stop}
	if _, err := p.Evaluate(200, nil, nil, func() bool { return stop }); err != cancelled || calls != 2 {
		t.Fatalf("%v calls=%d", err, calls)
	}
}
func TestEvaluationBoundsContextAndKeepsErrorsSafe(t *testing.T) {
	predicate, e := (Adapter{}).CompilePredicate(`response.body.SECRET_SENTINEL_FIELD = true`)
	if e != nil {
		t.Fatal(e)
	}
	if _, err := predicate.Evaluate(200, map[string]any{}, nil, nil); err != unavailable || strings.Contains(err.Error(), "SENTINEL") {
		t.Fatal("unsafe lookup error")
	}
	cycle := map[string]any{}
	cycle["self"] = cycle
	if _, err := predicate.Evaluate(200, cycle, nil, nil); err != contextLimit {
		t.Fatalf("unbounded cyclic context: %v", err)
	}
	if _, err := predicate.Evaluate(200, map[string]any{"large": strings.Repeat("x", 1024*1024+1)}, nil, nil); err != contextLimit {
		t.Fatalf("unbounded context: %v", err)
	}
	status, e := (Adapter{}).CompilePredicate(`response.status = 200`)
	if e != nil {
		t.Fatal(e)
	}
	if value, err := status.Evaluate(200, cycle, map[string]any{"ignored": func() {}}, nil); err != nil || !value {
		t.Fatalf("unused payload admitted: %v %v", value, err)
	}
	for _, number := range []json.Number{"NaN", "1e9999999999999999999999999", "1e-9999999999999999999999999999"} {
		if _, err := predicate.Evaluate(200, map[string]any{"SECRET_SENTINEL_FIELD": number}, nil, nil); err != mismatch {
			t.Fatalf("invalid numeric input: %v", err)
		}
	}
}
func TestCompiledPredicateHasIndependentConcurrentScopes(t *testing.T) {
	predicate, e := (Adapter{}).CompilePredicate(`response.body.value = inputs.expected`)
	if e != nil {
		t.Fatal(e)
	}
	var workers sync.WaitGroup
	for i := 0; i < 20; i++ {
		workers.Add(1)
		go func(i int) {
			defer workers.Done()
			value := i%2 == 0
			actual, err := predicate.Evaluate(200, map[string]any{"value": value}, map[string]any{"expected": value}, nil)
			if err != nil || !actual {
				t.Errorf("scope leaked: %v %v", actual, err)
			}
		}(i)
	}
	workers.Wait()
}

func TestExtractionUsesExternalEvaluationAndReturnsPlainScalars(t *testing.T) {
	for _, test := range []struct {
		value, want any
		failure     string
	}{
		{"token", "token", ""}, {true, true, ""}, {json.Number("3"), int64(3), ""}, {json.Number("1.0"), nil, "typeMismatch"},
		{json.Number("1.5"), nil, "typeMismatch"}, {json.Number("1e-60"), nil, "typeMismatch"}, {json.Number("9007199254740992"), nil, "typeMismatch"},
		{nil, nil, "nullValue"}, {map[string]any{}, nil, "typeMismatch"}, {strings.Repeat("x", 8193), nil, "valueLimit"},
	} {
		expression, e := (Adapter{}).CompileExtractionValue("response.body.value")
		if e != nil {
			t.Fatal(e)
		}
		actual, err := expression.Evaluate(map[string]any{"value": test.value}, nil)
		if test.failure != "" {
			if err == nil || err.Error() != test.failure {
				t.Fatalf("invalid extraction result: %v", err)
			}
		} else if err != nil || actual != test.want {
			t.Fatalf("%v/%v, want %v", actual, err, test.want)
		}
	}
	expression, e := (Adapter{}).CompileExtractionValue("response.body.token")
	if e != nil {
		t.Fatal(e)
	}
	if _, err := expression.Evaluate(map[string]any{}, nil); err != unavailable {
		t.Fatal(err)
	}
	if _, err := expression.Evaluate(map[string]any{"token": "fake"}, func() bool { return true }); err != cancelled {
		t.Fatal(err)
	}
}

func FuzzExternalEvaluationNeverPanics(f *testing.F) {
	for _, seed := range []struct{ source, body string }{{"response.body.value = 1", `{"value":1}`}, {"response.body.value != inputs.expected", `{"value":null}`}, {"response.body.value and true", `{"value":"yes"}`}, {"true", `{}`}, {"response.body.value > 0", `{"value":1e-999999999}`}} {
		f.Add(seed.source, seed.body)
	}
	f.Fuzz(func(t *testing.T, source, body string) {
		if len(source) > 4096 || len(body) > 20000 {
			t.Skip()
		}
		var value any
		decoder := json.NewDecoder(strings.NewReader(body))
		decoder.UseNumber()
		if decoder.Decode(&value) != nil {
			return
		}
		if predicate, e := (Adapter{}).CompilePredicate(source); e == nil {
			_, _ = predicate.Evaluate(200, value, map[string]any{"expected": true}, nil)
		}
		if extraction, e := (Adapter{}).CompileExtractionValue(source); e == nil {
			_, _ = extraction.Evaluate(value, nil)
		}
	})
}
