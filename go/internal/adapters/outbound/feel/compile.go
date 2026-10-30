package feel

import (
	"encoding/json"
	"sort"
	"strconv"
	"strings"

	library "github.com/pbinitiative/feel"
	"interleaving/internal/domain"
)

// Static type constraints describe compiler checks, not execution instructions.
// Unknown response-body types remain the external evaluator's responsibility.
type knownType struct{ kind, input string }
type typeConstraint struct {
	left, right       knownType
	boolean, ordering bool
}
type compiled struct {
	root        library.Node
	literals    library.Scope
	constraints []typeConstraint
	inputs      []string
	body        bool
	storage     int
}

// CompileExtractionValue retains the external property AST for extraction and
// bearer-token reads. BodyPath metadata never becomes a separate evaluator.
func (a Adapter) CompileExtractionValue(source string) (domain.ScalarExpression, *domain.ExpressionError) {
	path, e := a.CompileExtraction(source)
	if e != nil {
		return nil, e
	}
	ast, err := library.ParseString("response.body." + strings.Join(path, "."))
	if err != nil {
		return nil, &domain.ExpressionError{Code: "invalidExpression"}
	}
	return &scalarExpression{expression: &compiled{root: ast, body: true}}, nil
}

// CompilePredicate retains the external AST. The walk only admits supported
// syntax, collects dependencies/type constraints, and normalizes literal data.
// There is no internal expression tree or operator evaluator.
func (Adapter) CompilePredicate(source string) (domain.Predicate, *domain.ExpressionError) {
	admitted, e := admit(source)
	if e != nil {
		return nil, e
	}
	ast, err := library.ParseString(admitted.text)
	if err != nil {
		return nil, invalid(0)
	}
	result := &compiled{literals: library.Scope{}}
	names := map[string]bool{}
	count, comparisons := 0, 0
	operand := func(node library.Node, depth int) (library.Node, knownType, *domain.ExpressionError) {
		count++
		if depth > 32 || count > 256 {
			return nil, knownType{}, limit(0)
		}
		result.storage += 96
		switch n := node.(type) {
		case *library.BoolNode:
			return node, knownType{kind: "boolean"}, nil
		case *library.NumberNode:
			result.storage += len(n.Value)
			return node, knownType{kind: "integer"}, nil
		case *library.StringNode:
			// StringNode.Content decodes only a subset of JSON escapes. Bind the
			// admitted literal as a Go string; the library resolves it as a normal Var.
			var value string
			if json.Unmarshal([]byte(n.Value), &value) != nil {
				return nil, knownType{}, invalid(admitted.offset(node))
			}
			name := "$interl.literal." + strconv.Itoa(len(result.literals))
			result.literals[name] = value
			result.storage += len(name) + len(value) + 64
			return &library.Var{Name: name}, knownType{kind: "string"}, nil
		default:
			path, ok := propertyPath(node)
			if !ok {
				return nil, knownType{}, invalid(admitted.offset(node))
			}
			count += len(path)*2 - 2
			if depth+len(path)-1 > 32 || count > 256 {
				return nil, knownType{}, limit(0)
			}
			// DotOp stores its attribute outside Node; include those retained names.
			result.storage += (len(path) - 1) * 96
			for _, name := range path {
				result.storage += len(name)
			}
			if len(path) >= 3 && path[0] == "response" && path[1] == "body" {
				if _, e := (Adapter{}).CompileExtraction(strings.Join(path, ".")); e != nil {
					return nil, knownType{}, invalid(admitted.offset(node))
				}
				result.body = true
				return node, knownType{}, nil
			}
			if len(path) == 2 && path[0] == "response" && path[1] == "status" {
				return node, knownType{kind: "integer"}, nil
			}
			if len(path) == 2 && path[0] == "inputs" && !reserved(path[1]) {
				names[path[1]] = true
				return node, knownType{input: path[1]}, nil
			}
			return nil, knownType{}, invalid(admitted.offset(node))
		}
	}
	var inspect func(library.Node, int) (library.Node, *domain.ExpressionError)
	inspect = func(node library.Node, depth int) (library.Node, *domain.ExpressionError) {
		count++
		if depth > 30 || count > 256 {
			return nil, limit(0)
		}
		result.storage += 96
		if n, ok := node.(*library.Binop); ok {
			if n.Op == "and" || n.Op == "or" {
				left, e := inspect(n.Left, depth+1)
				if e != nil {
					return nil, e
				}
				right, e := inspect(n.Right, depth+1)
				if e != nil {
					return nil, e
				}
				n.Left = left
				n.Right = right
				return n, nil
			}
			if !isComparison(n.Op) {
				return nil, invalid(admitted.offset(node))
			}
			comparisons++
			if comparisons > 64 {
				return nil, limit(0)
			}
			left, lt, e := operand(n.Left, depth+1)
			if e != nil {
				return nil, e
			}
			right, rt, e := operand(n.Right, depth+1)
			if e != nil {
				return nil, e
			}
			n.Left = left
			n.Right = right
			result.constraints = append(result.constraints, typeConstraint{left: lt, right: rt, ordering: n.Op != "=" && n.Op != "!="})
			return n, nil
		}
		leaf, kind, e := operand(node, depth+1)
		if e != nil {
			return nil, e
		}
		result.constraints = append(result.constraints, typeConstraint{left: kind, boolean: true})
		return leaf, nil
	}
	result.root, e = inspect(ast, 1)
	if e != nil {
		return nil, e
	}
	for name := range names {
		result.inputs = append(result.inputs, name)
	}
	sort.Strings(result.inputs)
	return result, nil
}

func (c *compiled) InputNames() []string { return append([]string(nil), c.inputs...) }
func (c *compiled) RequiresBody() bool   { return c.body }
func (c *compiled) StorageBytes() int    { return c.storage }
func (c *compiled) ValidateInputTypes(inputs map[string]string) error {
	kind := func(k knownType) string {
		if k.input != "" {
			return inputs[k.input]
		}
		return k.kind
	}
	for _, check := range c.constraints {
		a, b := kind(check.left), kind(check.right)
		if check.boolean {
			if a != "" && a != "boolean" {
				return invalid(0)
			}
		} else if a != "" && b != "" && a != b || check.ordering && (a == "boolean" || b == "boolean") {
			return invalid(0)
		}
	}
	return nil
}
