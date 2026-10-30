// Package compatibility exercises the same frozen inputs as the frozen contract baseline.
package compatibility_test

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"interleaving/internal/adapters/inbound/cli"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/adapters/outbound/feel"
	"interleaving/internal/bootstrap"
)

const root = "../../.."

type validationExpected struct {
	ExitCode                     int
	Diagnostics                  []struct{ Category, Code, FieldPath string }
	StdoutPresent, StderrPresent bool
}

func assertValidation(t *testing.T, path string, expected validationExpected) {
	t.Helper()
	var out, stderr bytes.Buffer
	app := bootstrap.Workspace(charmlog.New(&stderr))
	code := cli.Run(context.Background(), []string{"validate", path}, &out, &stderr, app)
	if code != expected.ExitCode || out.Len() > 0 != expected.StdoutPresent || stderr.Len() > 0 != expected.StderrPresent {
		t.Fatalf("exit/output parity mismatch: %d out=%q err=%q", code, out.String(), stderr.String())
	}
	got := regexp.MustCompile(`\[([A-Za-z]+)/([^\]]+)\] ([^:]+):`).FindAllStringSubmatch(stderr.String(), -1)
	if len(got) != len(expected.Diagnostics) {
		t.Fatalf("diagnostic count: %s", stderr.String())
	}
	for i, d := range expected.Diagnostics {
		if !reflect.DeepEqual(got[i][1:], []string{d.Category, d.Code, d.FieldPath}) {
			t.Fatalf("diagnostic %v, want %+v", got[i][1:], d)
		}
	}
}

func TestExtendedScenarioCorpus(t *testing.T) {
	var corpus struct {
		Cases []struct {
			ID, Base, Format string
			Source, Repeat   *string
			Prefix, Suffix   string
			Count            int
			Patches          []struct {
				Path     []string
				Value    any
				RawValue *string
			}
			Expected validationExpected
		}
	}
	readJSON(t, "tests/compatibility/phase-2-scenarios.json", &corpus)
	for _, test := range corpus.Cases {
		t.Run(test.ID, func(t *testing.T) {
			var source []byte
			switch {
			case test.Source != nil:
				source = []byte(*test.Source)
			case test.Repeat != nil:
				source = []byte(test.Prefix + strings.Repeat(*test.Repeat, test.Count) + test.Suffix)
			default:
				var doc map[string]any
				readJSON(t, test.Base, &doc)
				for _, patch := range test.Patches {
					if patch.RawValue != nil {
						decoder := json.NewDecoder(strings.NewReader(*patch.RawValue))
						decoder.UseNumber()
						if e := decoder.Decode(&patch.Value); e != nil {
							t.Fatal(e)
						}
					}
					var parent any = doc
					for _, segment := range patch.Path[:len(patch.Path)-1] {
						switch p := parent.(type) {
						case map[string]any:
							if p[segment] == nil {
								p[segment] = map[string]any{}
							}
							parent = p[segment]
						case []any:
							i, _ := strconv.Atoi(segment)
							parent = p[i]
						}
					}
					last := patch.Path[len(patch.Path)-1]
					switch p := parent.(type) {
					case map[string]any:
						p[last] = patch.Value
					case []any:
						i, _ := strconv.Atoi(last)
						p[i] = patch.Value
					}
				}
				var err error
				source, err = json.Marshal(doc)
				if err != nil {
					t.Fatal(err)
				}
			}
			extension := "json"
			if test.Format == "yaml" {
				extension = "yaml"
			}
			path := filepath.Join(t.TempDir(), "scenario."+extension)
			if e := os.WriteFile(path, source, 0600); e != nil {
				t.Fatal(e)
			}
			assertValidation(t, path, test.Expected)
		})
	}
}

func readJSON(t *testing.T, path string, target any) {
	t.Helper()
	data, e := os.ReadFile(filepath.Join(root, path))
	if e != nil {
		t.Fatal(e)
	}
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	if e = dec.Decode(target); e != nil {
		t.Fatal(e)
	}
}
func TestFrozenOfflineValidationParity(t *testing.T) {
	var baseline struct {
		Validations []struct {
			File, SHA256                 string
			ExitCode                     int
			Diagnostics                  []struct{ Category, Code, FieldPath string }
			StdoutPresent, StderrPresent bool
		}
	}
	readJSON(t, "tests/compatibility/contract-baseline.json", &baseline)
	re := regexp.MustCompile(`\[([A-Za-z]+)/([^\]]+)\] ([^:]+):`)
	for _, fixture := range baseline.Validations {
		t.Run(fixture.File, func(t *testing.T) {
			path := filepath.Join(root, fixture.File)
			data, e := os.ReadFile(path)
			if e != nil {
				t.Fatal(e)
			}
			sum := sha256.Sum256(data)
			if hex.EncodeToString(sum[:]) != fixture.SHA256 {
				t.Fatal("frozen fixture changed")
			}
			var out, stderr bytes.Buffer
			app := bootstrap.Workspace(charmlog.New(&stderr))
			code := cli.Run(context.Background(), []string{"validate", path}, &out, &stderr, app)
			if code != fixture.ExitCode || out.Len() > 0 != fixture.StdoutPresent || stderr.Len() > 0 != fixture.StderrPresent {
				t.Fatalf("exit/output parity mismatch: %d, out=%q err=%q", code, out.String(), stderr.String())
			}
			got := re.FindAllStringSubmatch(stderr.String(), -1)
			if len(got) != len(fixture.Diagnostics) {
				t.Fatalf("diagnostic count: %s", stderr.String())
			}
			for i, d := range fixture.Diagnostics {
				if !reflect.DeepEqual(got[i][1:], []string{d.Category, d.Code, d.FieldPath}) {
					t.Fatalf("diagnostic: %v; want %+v", got[i][1:], d)
				}
			}
		})
	}
}

type expressionOutcome struct {
	Value        *bool
	Error        string
	CompileError bool
}

func TestSharedFEELCorpus(t *testing.T) {
	var corpus struct {
		Predicates []struct {
			ID, Expression string
			Status         uint16
			Body           any
			Inputs         map[string]any
			Cancelled      bool
			Expected       expressionOutcome
		}
		Extractions []struct {
			ID, Expression string
			Expected       struct {
				BodyPath     []string
				CompileError bool
			}
		}
	}
	readJSON(t, "tests/compatibility/expressions.json", &corpus)
	var extended = corpus
	extended.Predicates = nil
	extended.Extractions = nil
	readJSON(t, "tests/compatibility/expressions-extended.json", &extended)
	corpus.Predicates = append(corpus.Predicates, extended.Predicates...)
	corpus.Extractions = append(corpus.Extractions, extended.Extractions...)
	var native struct {
		Cases []struct {
			ID, Expression, Reason     string
			BaselineExpected, Expected expressionOutcome
		}
	}
	readJSON(t, "tests/compatibility/feel-library-outcomes.json", &native)
	outcomes := map[string]expressionOutcome{}
	used := map[string]bool{}
	for _, difference := range native.Cases {
		if difference.Reason == "" {
			t.Fatal("library outcome requires a reason")
		}
		found := false
		for _, test := range corpus.Predicates {
			if test.ID == difference.ID {
				if test.Expression != difference.Expression || !reflect.DeepEqual(test.Expected, difference.BaselineExpected) {
					t.Fatalf("stale library outcome for %s", test.ID)
				}
				found = true
			}
		}
		if !found {
			t.Fatalf("unknown library outcome %s", difference.ID)
		}
		if _, ok := outcomes[difference.ID]; ok {
			t.Fatal("duplicate library outcome")
		}
		outcomes[difference.ID] = difference.Expected
	}
	adapter := feel.Adapter{}
	for _, test := range corpus.Predicates {
		t.Run(test.ID, func(t *testing.T) {
			p, e := adapter.CompilePredicate(test.Expression)
			if (e != nil) != test.Expected.CompileError {
				t.Fatalf("compile: %v", e)
			}
			if e != nil {
				return
			}
			expected := test.Expected
			if native, ok := outcomes[test.ID]; ok {
				expected = native
				used[test.ID] = true
			}
			value, err := p.Evaluate(test.Status, test.Body, test.Inputs, func() bool { return test.Cancelled })
			if expected.Error != "" {
				if err == nil || err.Error() != expected.Error {
					t.Fatalf("runtime error %v", err)
				}
			} else if err != nil || expected.Value == nil || value != *expected.Value {
				t.Fatalf("value %v, error %v", value, err)
			}
		})
	}
	if len(used) != len(outcomes) {
		t.Fatal("not all documented library outcomes were exercised")
	}
	for _, test := range corpus.Extractions {
		t.Run("extract/"+test.ID, func(t *testing.T) {
			path, e := adapter.CompileExtraction(test.Expression)
			if (e != nil) != test.Expected.CompileError {
				t.Fatalf("compile: %v", e)
			}
			if e == nil && !reflect.DeepEqual(path, test.Expected.BodyPath) {
				t.Fatal(path)
			}
		})
	}
}
func TestEmbeddedScenarioSchemaMatchesSharedContract(t *testing.T) {
	a, e := os.ReadFile(filepath.Join(root, "schemas/scenario.schema.json"))
	if e != nil {
		t.Fatal(e)
	}
	b, e := os.ReadFile("../adapters/outbound/scenariocodec/scenario.schema.json")
	if e != nil {
		t.Fatal(e)
	}
	if !bytes.Equal(a, b) {
		t.Fatal("embedded codec schema must match schemas/scenario.schema.json")
	}
}
func TestValidationDoesNotExposeCredentialValues(t *testing.T) {
	t.Setenv("INTERL_ALICE_USERNAME", "SECRET_SENTINEL_USERNAME")
	t.Setenv("INTERL_ALICE_PASSWORD", "SECRET_SENTINEL_PASSWORD")
	var out, stderr bytes.Buffer
	code := cli.Run(context.Background(), []string{"validate", filepath.Join(root, "scenarios/examples/feature-showcase.interl.yaml")}, &out, &stderr, bootstrap.Workspace(charmlog.New(&stderr)))
	if code != 0 {
		t.Fatal(stderr.String())
	}
	if strings.Contains(out.String()+stderr.String(), "SECRET_SENTINEL") {
		t.Fatal("credential leak")
	}
}
