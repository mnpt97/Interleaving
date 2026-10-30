// Package architecture enforces inward-only dependencies, independently of a desktop build.
package architecture

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func TestCoreHasNoAdapterOrAmbientIOImports(t *testing.T) {
	for _, area := range []string{"domain", "application"} {
		files, err := filepath.Glob(filepath.Join("..", area, "*.go"))
		if err != nil {
			t.Fatal(err)
		}
		for _, name := range files {
			if strings.HasSuffix(name, "_test.go") {
				continue
			}
			file, err := parser.ParseFile(token.NewFileSet(), name, nil, 0)
			if err != nil {
				t.Fatal(err)
			}
			aliases := map[string]string{}
			for _, spec := range file.Imports {
				path, _ := strconv.Unquote(spec.Path.Value)
				alias := filepath.Base(path)
				if spec.Name != nil {
					alias = spec.Name.Name
				}
				if alias == "." {
					t.Errorf("%s uses a dot import", name)
				}
				aliases[alias] = path
				if strings.Contains(strings.Split(path, "/")[0], ".") {
					t.Errorf("%s imports third-party adapter %s", name, path)
				}
				if strings.HasPrefix(path, "interleaving/") && path != "interleaving/internal/domain" {
					t.Errorf("%s depends outward on %s", name, path)
				}
				for _, prefix := range []string{"os", "net", "log", "unsafe"} {
					if path == prefix || strings.HasPrefix(path, prefix+"/") || strings.HasSuffix(prefix, "/") && strings.HasPrefix(path, prefix) {
						t.Errorf("%s imports effect/adapter %s", name, path)
					}
				}
			}
			ast.Inspect(file, func(node ast.Node) bool {
				call, ok := node.(*ast.CallExpr)
				if !ok {
					return true
				}
				if fn, ok := call.Fun.(*ast.Ident); ok && (fn.Name == "print" || fn.Name == "println") {
					t.Errorf("%s writes ambient diagnostic output", name)
				}
				sel, ok := call.Fun.(*ast.SelectorExpr)
				if !ok {
					return true
				}
				id, ok := sel.X.(*ast.Ident)
				if ok && aliases[id.Name] == "time" {
					switch sel.Sel.Name {
					case "Now", "Sleep", "After", "AfterFunc", "NewTimer", "NewTicker", "Tick":
						t.Errorf("%s uses ambient time.%s", name, sel.Sel.Name)
					}
				}
				if ok && aliases[id.Name] == "fmt" {
					switch sel.Sel.Name {
					case "Print", "Println", "Printf":
						t.Errorf("%s bypasses the logging port", name)
					}
				}
				return true
			})
		}
	}
}
func TestCLIHasNoDesktopDependency(t *testing.T) {
	command := exec.Command("go", "list", "-deps", "./cmd/interl-go")
	command.Dir = "../.."
	result, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("%v: %s", err, result)
	}
	for _, line := range strings.Split(string(result), "\n") {
		if strings.Contains(line, "wails") || strings.Contains(line, "desktop-assets") || line == "runtime/cgo" {
			t.Errorf("CLI depends on desktop/CGO: %s", line)
		}
	}
}
