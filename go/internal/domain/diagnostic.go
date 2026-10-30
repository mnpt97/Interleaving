package domain

import "strings"

// Diagnostic is a safe public failure. It never retains parser/library errors or
// runtime payloads. Document and expression locations use separate coordinates.
type Diagnostic struct {
	Category         string  `json:"category"`
	Code             string  `json:"code"`
	Message          string  `json:"message"`
	FieldPath        string  `json:"fieldPath"`
	NodeID           *string `json:"nodeId"`
	WorkflowID       *string `json:"workflowId,omitempty"`
	FilePath         *string `json:"filePath"`
	Line             *int    `json:"line"`
	Column           *int    `json:"column"`
	ExpressionLine   *int    `json:"expressionLine,omitempty"`
	ExpressionColumn *int    `json:"expressionColumn,omitempty"`
}

func Problem(category, code, path, message string) Diagnostic {
	if path == "" {
		path = "$"
	}
	return Diagnostic{Category: category, Code: code, FieldPath: path, Message: message}
}
func (d Diagnostic) Error() string {
	return d.Category + "/" + d.Code + " " + d.FieldPath + ": " + d.Message
}
func Pointer(path, key string) string {
	return path + "/" + strings.ReplaceAll(strings.ReplaceAll(key, "~", "~0"), "/", "~1")
}

// Diagnostics allows use cases to return multiple errors through Go's error API.
type Diagnostics []Diagnostic

func (d Diagnostics) Error() string {
	if len(d) == 0 {
		return ""
	}
	return d[0].Error()
}

// ExpressionError records a byte offset without retaining rejected source text.
type ExpressionError struct {
	Code   string
	Offset int
}

func (e *ExpressionError) Error() string { return e.Code }

// Predicate and Expressions form the replaceable bounded FEEL contract. The
// application injects the adapter; domain compilation has no parser dependency.
type Predicate interface {
	Evaluate(status uint16, body any, inputs map[string]any, interrupted func() bool) (bool, error)
	InputNames() []string
	RequiresBody() bool
	ValidateInputTypes(map[string]string) error
	StorageBytes() int
}
type Expressions interface {
	CompilePredicate(string) (Predicate, *ExpressionError)
	CompileExtraction(string) ([]string, *ExpressionError)
	CompileExtractionValue(string) (ScalarExpression, *ExpressionError)
}

// ScalarExpression evaluates an admitted extraction with library types confined
// to its adapter. Outputs must fit the project's string/integer/boolean ports.
type ScalarExpression interface {
	Evaluate(body any, interrupted func() bool) (any, error)
}

// TargetURLs supplies pure URL admission/normalization without exposing a parser
// library or network client to the domain. Its output is a canonical base URL.
type TargetURLs interface{ BaseURL(string) (string, bool) }
