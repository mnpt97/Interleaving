package application

import (
	"bytes"
	"context"
	"encoding/json"
	"interleaving/internal/domain"
	"strings"
)

func (r *runner) scalar(source domain.Source, f *frame, init map[string]any) (any, *Failure) {
	var value any
	switch source.Source {
	case "literal":
		value = source.Value
	case "environment":
		value = r.program.Document.Environment.Variables[source.Name]
	case "identity":
		id := r.program.Document.Groups[0].IdentityAssignment.IdentityID
		if id != nil {
			value = r.credentials[*id+"\x00"+source.Field]
		}
	case "lifecycle":
		value = init[source.Name]
	default:
		if f != nil {
			value = f.values[domain.SourceKey(source)]
		}
	}
	if domain.ScalarType(value) == "" {
		return nil, fail("valueNotAvailable", "Required scalar value is unavailable")
	}
	return value, nil
}
func (r *runner) binding(b domain.Binding, parent *frame, init map[string]any) (map[string]any, *Failure) {
	flow := r.flowIndex(b.WorkflowID)
	if flow < 0 {
		return nil, fail("invalidProgram", "Workflow is unavailable")
	}
	values := map[string]any{}
	for _, name := range orderedKeys(r.program.Document.Workflows[flow].Inputs) {
		port := r.program.Document.Workflows[flow].Inputs[name]
		var v any
		if src, ok := b.Inputs[name]; ok {
			value, err := r.scalar(src, parent, init)
			if err != nil {
				return nil, err
			}
			v = value
		} else {
			decoder := json.NewDecoder(bytes.NewReader(port.Default))
			decoder.UseNumber()
			if decoder.Decode(&v) != nil {
				return nil, fail("missingInput", "Required workflow input is unavailable")
			}
		}
		if domain.ScalarType(v) != port.Type {
			return nil, fail("portTypeMismatch", "Workflow input has an invalid scalar type or size")
		}
		values[name] = v
		if text, ok := v.(string); ok {
			r.security.remember(text)
		}
	}
	pending := map[string]any{}
	for name, value := range init {
		pending["init:"+name] = value
	}
	for name, value := range values {
		pending["input:"+name] = value
	}
	if problem := r.capacity(pending); problem != nil {
		return nil, problem
	}
	return values, nil
}
func (r *runner) template(source domain.Source, f *frame, depth int, count *int) (any, *Failure) {
	if err := r.boundary(); err != nil {
		return nil, err
	}
	*count++
	if depth > 32 || *count > 256 {
		return nil, fail("templateLimit", "Template composition exceeds runtime limits")
	}
	switch source.Source {
	case "literal":
		return source.Value, nil
	case "object":
		m := map[string]any{}
		for _, name := range orderedKeys(source.Fields) {
			v, err := r.template(source.Fields[name], f, depth+1, count)
			if err != nil {
				return nil, err
			}
			m[name] = v
		}
		return m, nil
	case "array":
		m := []any{}
		for _, src := range source.Items {
			v, err := r.template(src, f, depth+1, count)
			if err != nil {
				return nil, err
			}
			m = append(m, v)
		}
		return m, nil
	default:
		return r.scalar(source, f, nil)
	}
}
func (r *runner) httpInput(config domain.Config, f *frame) (HTTPInput, *Failure) {
	doc := r.program.Document
	profile := doc.Groups[0].ClientProfileID
	base := ""
	for _, p := range doc.ClientProfiles {
		if p.ID == profile {
			base = r.program.Targets[p.BaseURL]
		}
	}
	if config.BaseURL != nil {
		base = r.program.Targets[*config.BaseURL]
	}
	path := config.Path
	var expanded strings.Builder
	for {
		start := strings.Index(path, "${")
		if start < 0 {
			expanded.WriteString(path)
			break
		}
		expanded.WriteString(path[:start])
		rest := path[start+2:]
		end := strings.Index(rest, "}")
		if end < 0 || !domain.Identifier(rest[:end]) {
			return HTTPInput{}, fail("configuration", "Invalid path variable")
		}
		value, ok := doc.Environment.Variables[rest[:end]]
		if !ok {
			return HTTPInput{}, fail("configuration", "Path variable is missing")
		}
		var text string
		switch v := value.(type) {
		case string:
			text = v
		case json.Number, bool:
			wire, _ := json.Marshal(v)
			text = string(wire)
		default:
			return HTTPInput{}, fail("configuration", "Path variable must be scalar")
		}
		if text == "." || text == ".." {
			return HTTPInput{}, fail("configuration", "Path variable cannot be a dot segment")
		}
		expanded.WriteString(r.workspace.runs.URLs.Encode(text))
		if expanded.Len() > 1048576 {
			return HTTPInput{}, fail("configuration", "Expanded path exceeds limits")
		}
		path = rest[end+1:]
	}
	input := HTTPInput{BaseURL: base, Path: expanded.String(), Method: config.Method, Headers: config.Headers}
	if config.TimeoutMS != nil {
		input.TimeoutMS = *config.TimeoutMS
		if *config.TimeoutMS == 0 {
			return HTTPInput{}, fail("configuration", "Timeout must be greater than zero")
		}
	}
	if config.Body != nil {
		body := config.Body
		switch body.Kind {
		case "text":
			text, ok := body.Value.(string)
			if !ok {
				return input, fail("configuration", "Text request body is invalid")
			}
			input.Body = []byte(text)
		case "json":
			input.Body, _ = jsonWire(body.Value)
			input.ContentType = "application/json"
			input.JSONBody = true
		case "jsonTemplate":
			if f != nil {
				fields := map[string]any{}
				count := 1
				for _, name := range orderedKeys(body.Fields) {
					value, err := r.template(body.Fields[name], f, 2, &count)
					if err != nil {
						return input, err
					}
					fields[name] = value
				}
				input.Body, _ = jsonWire(fields)
				input.ContentType = "application/json"
				input.JSONBody = true
			}
		}
	}
	return input, nil
}
func (r *runner) capacity(extra map[string]any) *Failure {
	count, values, responses := len(extra), 0, 0
	for _, value := range extra {
		encoded, _ := jsonWire(value)
		values += len(encoded)
	}
	for _, f := range r.frames {
		count += len(f.values)
		for _, value := range f.values {
			encoded, _ := jsonWire(value)
			values += len(encoded)
		}
		for _, response := range f.responses {
			responses += len(response.response.Body)
		}
	}
	if count > 128 || values > 65536 || responses > 1048576 {
		return fail("valueLimit", "Aggregate invocation storage exceeds its resource budget")
	}
	return nil
}
func (r *runner) flowIndex(id string) int {
	for i, w := range r.program.Document.Workflows {
		if w.ID == id {
			return i
		}
	}
	return -1
}
func (r *runner) response(f *frame, node string) (any, *Failure) {
	value := f.responses[node]
	if value == nil {
		return nil, fail("extractionResponseUnavailable", "Response data is unavailable")
	}
	if value.response.Truncated {
		return nil, fail("extractionResponseTruncated", "Response body was truncated")
	}
	if value.decoded {
		return value.parsed, nil
	}
	parsed, err := r.workspace.runs.JSON.Parse(r.activeContext, value.response.Body)
	if err != nil {
		if boundary := r.boundary(); boundary != nil {
			return nil, boundary
		}
		return nil, err
	}
	value.parsed = parsed
	value.decoded = true
	return parsed, nil
}
func (r *runner) expressionFailure(err error, route bool) *Failure {
	kind := "extractionWrongType"
	if route {
		kind = "routeTypeMismatch"
	}
	switch err.Error() {
	case "nullValue":
		return fail("extractionNullValue", "")
	case "valueLimit":
		return fail("extractionValueLimit", "")
	case "cancelled":
		return r.boundaryOrCancelled()
	case "expressionLimit":
		return fail("valueLimit", "Expression context exceeds limits")
	case "unavailable":
		kind = "extractionMissingProperty"
		if route {
			kind = "routeSourceUnavailable"
		}
	}
	return fail(kind, "Expression data is unavailable or has an invalid type")
}
func (r *runner) boundaryOrCancelled() *Failure {
	if err := r.boundary(); err != nil {
		return err
	}
	return fail("cancelled", "Run cancelled")
}
func expressionStop(ctx context.Context) func() bool { return func() bool { return ctx.Err() != nil } }
