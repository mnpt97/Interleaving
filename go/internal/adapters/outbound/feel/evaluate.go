package feel

import (
	"encoding/json"
	"errors"
	"math"
	"math/big"
	"strconv"
	"unicode/utf8"

	library "github.com/pbinitiative/feel"
	"interleaving/internal/domain"
)

var cancelled = errors.New("cancelled")
var unavailable = errors.New("unavailable")
var mismatch = errors.New("typeMismatch")
var contextLimit = errors.New("expressionLimit")

type evaluation struct {
	interrupted   func() bool
	failure       error
	values, bytes int
	scalar        bool
}

// A null context delegates property access to the external DotOp evaluator,
// while retaining the scalar-port diagnostic for an intermediate JSON null.
type nullContext struct{ state *evaluation }

func (n *nullContext) GetAttr(string) (any, bool) {
	n.state.failure = errors.New("nullValue")
	return nil, false
}

func (e *evaluation) stop() bool {
	if e.interrupted != nil && e.interrupted() {
		e.failure = cancelled
	}
	return e.failure != nil
}

// value adapts JSON types to the library's numeric/context representation. It
// does not resolve property paths or apply FEEL operators. Only plain JSON data
// enters the interpreter; caller-supplied objects/functions are never exposed.
func (e *evaluation) value(value any, depth int) (any, error) {
	if e.stop() {
		return nil, e.failure
	}
	switch v := value.(type) {
	case int:
		value = json.Number(strconv.Itoa(v))
	case int64:
		value = json.Number(strconv.FormatInt(v, 10))
	case uint64:
		value = json.Number(strconv.FormatUint(v, 10))
	case float64:
		if math.IsInf(v, 0) || math.IsNaN(v) {
			return nil, mismatch
		}
		value = json.Number(strconv.FormatFloat(v, 'g', -1, 64))
	}
	e.values++
	if depth > 64 || e.values > 100000 || e.bytes > 1024*1024 {
		return nil, contextLimit
	}
	switch v := value.(type) {
	case nil:
		if e.scalar {
			return &nullContext{state: e}, nil
		}
		return nil, nil
	case bool:
		return value, nil
	case string:
		if !utf8.ValidString(v) {
			return nil, mismatch
		}
		e.bytes += len(v)
		if e.bytes > 1024*1024 {
			return nil, contextLimit
		}
		return v, nil
	case json.Number:
		if len(v) > 128 || !json.Valid([]byte(v)) {
			return nil, mismatch
		}
		number, err := strconv.ParseFloat(string(v), 64)
		if err != nil || math.IsInf(number, 0) {
			return nil, mismatch
		}
		if _, _, err := big.ParseFloat(string(v), 10, library.Prec, big.ToNearestEven); err != nil {
			return nil, mismatch
		}
		e.bytes += len(v)
		if e.bytes > 1024*1024 {
			return nil, contextLimit
		}
		// Extraction admits property access only. Preserve the original JSON
		// number type so 1.0 cannot silently become an integer workflow output.
		if e.scalar {
			return v, nil
		}
		return library.NewNumber(string(v)), nil
	case map[string]any:
		if len(v) > 100000 {
			return nil, contextLimit
		}
		result := make(map[string]any, len(v))
		for name, value := range v {
			if !utf8.ValidString(name) {
				return nil, mismatch
			}
			e.bytes += len(name)
			converted, err := e.value(value, depth+1)
			if err != nil {
				return nil, err
			}
			result[name] = converted
		}
		return result, nil
	case []any:
		if len(v) > 100000 {
			return nil, contextLimit
		}
		result := make([]any, len(v))
		for i, value := range v {
			converted, err := e.value(value, depth+1)
			if err != nil {
				return nil, err
			}
			result[i] = converted
		}
		return result, nil
	default:
		return nil, mismatch
	}
}

// Evaluate delegates the complete expression to pbinitiative/feel. Each call
// gets its own interpreter and scope; the compiled AST contains no runtime data.
// Cancellation is checked during context admission and before/after evaluation.
// The upstream evaluator has no context hook; AST/payload bounds cap its work.
func (c *compiled) evaluate(status uint16, body any, inputs map[string]any, interrupted func() bool, scalar bool) (any, error) {
	state := &evaluation{interrupted: interrupted, scalar: scalar}
	if state.stop() {
		return false, state.failure
	}
	scope := make(library.Scope, len(c.literals)+2)
	for name, value := range c.literals {
		scope[name] = value
	}
	boundInputs := make(map[string]any, len(c.inputs))
	for _, name := range c.inputs {
		if value, ok := inputs[name]; ok {
			boundInputs[name] = value
		}
	}
	convertedInputs, err := state.value(boundInputs, 0)
	if err != nil {
		return false, err
	}
	scope["inputs"] = convertedInputs
	var responseBody any
	if c.body {
		responseBody, err = state.value(body, 0)
		if err != nil {
			return false, err
		}
	}
	scope["response"] = map[string]any{"status": library.NewNumberFromInt64(int64(status)), "body": responseBody}
	interpreter := library.NewIntepreter()
	interpreter.Push(scope)
	if state.stop() {
		return false, state.failure
	}
	value, err := c.root.Eval(interpreter)
	if state.stop() {
		return false, state.failure
	}
	if err != nil {
		// Never return library error strings: they can include referenced field names
		// or other private data. Codes describe lookup failures without formatting it.
		var failure *library.EvalError
		if errors.As(err, &failure) && (failure.Code == -4000 || failure.Code == -4002) {
			return false, unavailable
		}
		return false, mismatch
	}
	return value, nil
}

func (c *compiled) Evaluate(status uint16, body any, inputs map[string]any, interrupted func() bool) (bool, error) {
	value, err := c.evaluate(status, body, inputs, interrupted, false)
	if err != nil {
		return false, err
	}
	result, ok := value.(bool)
	if !ok {
		return false, mismatch
	}
	return result, nil
}

type scalarExpression struct{ expression *compiled }

// Extraction results cross the port only as supported Go scalars. Integer
// exactness is checked by the library; no decimal formatting/rounding is used.
func (s *scalarExpression) Evaluate(body any, interrupted func() bool) (any, error) {
	value, err := s.expression.evaluate(0, body, nil, interrupted, true)
	if err != nil {
		return nil, err
	}
	if number, ok := value.(*library.Number); ok {
		integer := number.Int64()
		if _, safe := domain.Integer(integer); !safe || !number.Equal(*library.NewNumberFromInt64(integer)) {
			return nil, mismatch
		}
		return integer, nil
	}
	if number, ok := value.(json.Number); ok {
		if integer, safe := domain.Integer(number); safe {
			return integer, nil
		}
		return nil, mismatch
	}
	_, libraryNull := value.(*library.NullValue)
	_, jsonNull := value.(*nullContext)
	if value == nil || libraryNull || jsonNull {
		return nil, errors.New("nullValue")
	}
	if text, ok := value.(string); ok && len(text) > 8192 {
		return nil, errors.New("valueLimit")
	}
	if domain.ScalarType(value) == "" {
		return nil, mismatch
	}
	return value, nil
}
