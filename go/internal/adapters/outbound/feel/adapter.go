// Package feel admits a bounded pbinitiative/feel AST and delegates evaluation
// to the same library. Library types remain inside this driven adapter.
package feel

import (
	"encoding/json"
	"strconv"
	"strings"
	"unicode/utf8"

	parser "github.com/pbinitiative/feel"
	"interleaving/internal/domain"
)

type Adapter struct{}

var _ domain.Expressions = Adapter{}

func invalid(offset int) *domain.ExpressionError {
	return &domain.ExpressionError{Code: "invalidRouteCondition", Offset: offset}
}
func limit(offset int) *domain.ExpressionError {
	return &domain.ExpressionError{Code: "expressionLimit", Offset: offset}
}
func identifier(s string) bool { return domain.Identifier(s) }
func reserved(s string) bool {
	return strings.Contains("|and|or|not|if|then|else|for|return|some|every|satisfies|in|between|instance|of|function|external|true|false|null|", "|"+s+"|")
}

// CompileExtraction admits one bounded property path and verifies the library
// AST preserves exactly that path. Names cannot acquire general FEEL semantics.
func (Adapter) CompileExtraction(source string) ([]string, *domain.ExpressionError) {
	if len(source) > 4096 {
		return nil, limit(4096)
	}
	names := []string{}
	offset := 0
	for _, part := range strings.Split(source, ".") {
		name := strings.Trim(part, " \t\n\r\v\f")
		at := offset + len(part) - len(strings.TrimLeft(part, " \t\n\r\v\f"))
		if len(names) == 32 || len(name) > 128 {
			return nil, limit(at)
		}
		if name == "" {
			return nil, &domain.ExpressionError{Code: "invalidExpression", Offset: at}
		}
		if !identifier(name) {
			bad := 0
			for i, b := range []byte(name) {
				if !(b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b == '_' || i > 0 && b >= '0' && b <= '9') {
					bad = i
					break
				}
			}
			return nil, &domain.ExpressionError{Code: "unsupportedExpression", Offset: at + bad}
		}
		if reserved(name) {
			return nil, &domain.ExpressionError{Code: "unsupportedExpression", Offset: at}
		}
		names = append(names, name)
		offset += len(part) + 1
	}
	if len(names) < 3 || names[0] != "response" || names[1] != "body" {
		return nil, &domain.ExpressionError{Code: "unsupportedExpression"}
	}
	ast, err := parser.ParseString(strings.Join(names, "."))
	if err != nil {
		return nil, &domain.ExpressionError{Code: "invalidExpression"}
	}
	actual, ok := propertyPath(ast)
	if !ok || strings.Join(actual, ".") != strings.Join(names, ".") {
		return nil, &domain.ExpressionError{Code: "unsupportedExpression"}
	}
	return actual[2:], nil
}

func propertyPath(node parser.Node) ([]string, bool) {
	reversed := []string{}
	for {
		switch n := node.(type) {
		case *parser.DotOp:
			if !identifier(n.Attr) || len(reversed) >= 31 {
				return nil, false
			}
			reversed = append(reversed, n.Attr)
			node = n.Left
		case *parser.Var:
			if !identifier(n.Name) {
				return nil, false
			}
			names := []string{n.Name}
			for i := len(reversed) - 1; i >= 0; i-- {
				names = append(names, reversed[i])
			}
			return names, true
		default:
			return nil, false
		}
	}
}

type admittedToken struct {
	kind, value string
	offset      int
}
type admittedSource struct {
	text             string
	tokens           []admittedToken
	canonicalOffsets []int
}

func whitespaceOnly(s string) bool { return strings.Trim(s, " \t\n\r\v\f") == "" }

// admit uses the library scanner, not a second tokenizer or grammar parser.
// Resource checks run before parsing. Gaps must be whitespace because the
// library scanner otherwise silently skips comments outside the current subset.
func admit(source string) (admittedSource, *domain.ExpressionError) {
	if len(source) > 4096 {
		return admittedSource{}, limit(4096)
	}
	if !utf8.ValidString(source) {
		return admittedSource{}, invalid(0)
	}
	scanner := parser.NewScanner(strings.ReplaceAll(source, "\v", " "))
	tokens := []admittedToken{}
	previous, count, depth := 0, 0, 0
	for {
		if err := scanner.Next(); err != nil {
			return admittedSource{}, invalid(previous)
		}
		t := scanner.Current()
		start := scanner.Eaten - len(t.Value)
		if !whitespaceOnly(source[previous:start]) {
			return admittedSource{}, invalid(previous)
		}
		previous = scanner.Eaten
		if t.Kind == parser.TokenEOF {
			break
		}
		count++
		if t.Kind == parser.TokenNumber && strings.HasPrefix(t.Value, "-") {
			count++
		}
		if count > 512 {
			return admittedSource{}, limit(start)
		}
		value := source[start:scanner.Eaten]
		switch t.Kind {
		case parser.TokenName:
			if !identifier(value) {
				return admittedSource{}, invalid(start)
			}
		case parser.TokenKeyword:
			if value != "true" && value != "false" && value != "and" && value != "or" {
				return admittedSource{}, invalid(start)
			}
		case parser.TokenNumber:
			digits := strings.TrimPrefix(value, "-")
			v, e := strconv.ParseInt(value, 10, 64)
			if !onlyDigits(digits) || len(digits) > 16 || len(digits) > 1 && digits[0] == '0' || e != nil || v > 9007199254740991 || v < -9007199254740991 {
				return admittedSource{}, invalid(start)
			}
		case parser.TokenString:
			var decoded string
			if !validSurrogates(value) || json.Unmarshal([]byte(value), &decoded) != nil {
				return admittedSource{}, invalid(start)
			}
		case "(":
			depth++
			if depth > 29 {
				return admittedSource{}, limit(0)
			}
		case ")":
			depth--
			if depth < 0 {
				return admittedSource{}, invalid(start)
			}
		case ".", "=", "!=", "<", "<=", ">", ">=", "-":
		default:
			return admittedSource{}, invalid(start)
		}
		tokens = append(tokens, admittedToken{t.Kind, value, start})
	}
	if len(tokens) == 0 || depth != 0 {
		return admittedSource{}, invalid(0)
	}
	result := admittedSource{}
	var text strings.Builder
	appendToken := func(t admittedToken) {
		if text.Len() > 0 {
			text.WriteByte(' ')
		}
		result.canonicalOffsets = append(result.canonicalOffsets, text.Len())
		result.tokens = append(result.tokens, t)
		text.WriteString(t.value)
	}
	for i := 0; i < len(tokens); i++ {
		t := tokens[i]
		if isComparison(t.kind) && (i > 0 && tokens[i-1].kind == ")" || i+1 < len(tokens) && tokens[i+1].kind == "(") {
			return admittedSource{}, invalid(t.offset)
		}
		// The reference admits whitespace between a unary minus and integer. The
		// library represents negative integers as one NumberNode, so normalize only
		// that token pair; subtraction and general unary operators remain forbidden.
		if t.kind == "-" {
			if i+1 >= len(tokens) || tokens[i+1].kind != parser.TokenNumber || strings.HasPrefix(tokens[i+1].value, "-") {
				return admittedSource{}, invalid(t.offset)
			}
			i++
			t.kind = parser.TokenNumber
			t.value = "-" + tokens[i].value
		}
		if t.kind == parser.TokenName {
			// FEEL names may contain spaces and keywords. This parser has no
			// synthetic scope parameter. Group
			// already-admitted ASCII property tokens to make their boundary explicit,
			// so `inputs.minimum and inputs.label` cannot become one qualified name.
			appendToken(admittedToken{kind: "(", value: "(", offset: t.offset})
			appendToken(t)
			names := 1
			for i+2 < len(tokens) && tokens[i+1].kind == "." && tokens[i+2].kind == parser.TokenName {
				names++
				if names > 32 {
					return admittedSource{}, limit(tokens[i+2].offset)
				}
				appendToken(tokens[i+1])
				appendToken(tokens[i+2])
				i += 2
			}
			appendToken(admittedToken{kind: ")", value: ")", offset: tokens[i].offset})
		} else {
			appendToken(t)
		}
	}
	result.text = text.String()
	return result, nil
}
func onlyDigits(s string) bool {
	if s == "" {
		return false
	}
	for _, b := range []byte(s) {
		if b < '0' || b > '9' {
			return false
		}
	}
	return true
}
func isComparison(s string) bool {
	switch s {
	case "=", "!=", "<", "<=", ">", ">=":
		return true
	}
	return false
}
func (s admittedSource) offset(node parser.Node) int {
	at := node.TextRange().Start.Column
	result := 0
	for i, p := range s.canonicalOffsets {
		if p > at {
			break
		}
		result = s.tokens[i].offset
	}
	return result
}
