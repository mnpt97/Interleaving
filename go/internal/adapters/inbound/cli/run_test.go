package cli_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"interleaving/internal/adapters/inbound/cli"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/bootstrap"
)

func TestRunCLIWritesPassedAndCancelledReports(t *testing.T) {
	for _, cancelled := range []bool{false, true} {
		t.Run(map[bool]string{false: "passed", true: "cancelled"}[cancelled], func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if cancelled {
					cancel()
					<-r.Context().Done()
					return
				}
				_, _ = w.Write([]byte(`{"value":9007199254740993}`))
			}))
			defer server.Close()
			source, err := os.ReadFile("../../../../../scenarios/examples/hello-api.interl.json")
			if err != nil {
				t.Fatal(err)
			}
			source = bytes.ReplaceAll(source, []byte("http://127.0.0.1:8080"), []byte(server.URL))
			dir := t.TempDir()
			input := filepath.Join(dir, "scenario.interl.json")
			output := filepath.Join(dir, "report.json")
			if err = os.WriteFile(input, source, 0600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			app := bootstrap.Workspace(charmlog.New(&stderr))
			code := cli.Run(ctx, []string{"run", input, "--output", output}, &stdout, &stderr, app)
			want := 0
			if cancelled {
				want = 130
			}
			if code != want {
				t.Fatalf("exit=%d stderr=%s", code, stderr.String())
			}
			data, err := os.ReadFile(output)
			if err != nil {
				t.Fatal(err)
			}
			var report map[string]any
			if err = json.Unmarshal(data, &report); err != nil {
				t.Fatal(err)
			}
			if report["reportVersion"] != float64(3) || report["status"] != map[bool]string{false: "passed", true: "cancelled"}[cancelled] {
				t.Fatal("report contract")
			}
			if !strings.Contains(stderr.String(), "run finished") || !strings.Contains(stdout.String(), "run.finished") {
				t.Fatal("logging/event separation")
			}
			if !cancelled && !bytes.Contains(data, []byte("9007199254740993")) {
				t.Fatal("response number lost precision")
			}
			stdout.Reset()
			stderr.Reset()
			if code = cli.Run(context.Background(), []string{"run", input, "--output", input}, &stdout, &stderr, app); code != 2 || stdout.Len() != 0 {
				t.Fatal("input overwrite not rejected before run")
			}
		})
	}
}
