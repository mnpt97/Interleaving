package cli_test

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"

	"interleaving/internal/adapters/inbound/cli"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/bootstrap"
)

func TestInvalidRunDoesNotCreateSuccessOrProbeExecution(t *testing.T) {
	for _, command := range []string{"run"} {
		var out, errout bytes.Buffer
		app := bootstrap.Workspace(charmlog.New(&errout))
		code := cli.Run(context.Background(), []string{command, "anything"}, &out, &errout, app)
		if code != 2 || out.Len() != 0 || errout.Len() == 0 {
			t.Fatalf("%s returned false capability", command)
		}
	}
}
func TestProbeSeparatesDiagnosticsFromJSONOutput(t *testing.T) {
	var out, errout bytes.Buffer
	app := bootstrap.Workspace(charmlog.New(&errout))
	if code := cli.Run(context.Background(), []string{"probe"}, &out, &errout, app); code != 0 {
		t.Fatal(code)
	}
	decoder := json.NewDecoder(&out)
	for i := 0; i < 3; i++ {
		var value map[string]any
		if err := decoder.Decode(&value); err != nil {
			t.Fatal(err)
		}
		if value["synthetic"] != true {
			t.Fatal("not a synthetic event")
		}
	}
	if !strings.Contains(errout.String(), "bridge probe completed") {
		t.Fatal("logging port not wired")
	}
}
