package reportfiles_test

import (
	"context"
	"interleaving/internal/adapters/outbound/reportfiles"
	"os"
	"path/filepath"
	"testing"
)

func TestReportNeverOverwritesInputAliasAndIsPrivate(t *testing.T) {
	dir := t.TempDir()
	input := filepath.Join(dir, "scenario.json")
	if err := os.WriteFile(input, []byte("original"), 0600); err != nil {
		t.Fatal(err)
	}
	a := reportfiles.Adapter{}
	for _, name := range []string{"same", "symlink", "hardlink"} {
		output := input
		if name == "symlink" {
			output = filepath.Join(dir, name)
			if err := os.Symlink(input, output); err != nil {
				t.Fatal(err)
			}
		}
		if name == "hardlink" {
			output = filepath.Join(dir, name)
			if err := os.Link(input, output); err != nil {
				t.Fatal(err)
			}
		}
		if err := a.Write(context.Background(), input, output, []byte("changed")); err == nil {
			t.Errorf("overwrote %s", name)
		}
	}
	data, _ := os.ReadFile(input)
	if string(data) != "original" {
		t.Fatal("input modified")
	}
	output := filepath.Join(dir, "report.json")
	if err := a.Write(context.Background(), input, output, []byte(`{"reportVersion":3}`)); err != nil {
		t.Fatal(err)
	}
	info, err := os.Stat(output)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0600 {
		t.Fatal("report not private")
	}
	files, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 4 {
		t.Fatal("temporary file leaked")
	}
}
