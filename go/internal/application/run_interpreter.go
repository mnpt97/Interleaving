package application

import (
	"interleaving/internal/domain"
	"strings"
)

func (r *runner) root(binding domain.Binding, init map[string]any) (map[string]any, *Failure) {
	if problem := r.emit(nil, -1, -1, "lifecycle.started", map[string]any{"phase": r.phase}, false); problem != nil {
		return nil, problem
	}
	inputs, problem := r.binding(binding, nil, init)
	var outputs map[string]any
	if problem == nil {
		outputs, problem = r.invoke(r.flowIndex(binding.WorkflowID), inputs, nil, nil)
	}
	_ = r.emit(nil, -1, -1, "lifecycle.finished", map[string]any{"phase": r.phase, "status": status(problem)}, true)
	return outputs, problem
}
func (r *runner) invoke(flow int, inputs map[string]any, parent, caller *string) (outputs map[string]any, problem *Failure) {
	r.invocations++
	if r.invocations > 128 || len(r.frames) >= 8 {
		return nil, fail("invocationLimit", "Workflow invocation limit exceeded")
	}
	w := r.program.Document.Workflows[flow]
	plan := r.program.Flows[flow]
	f := &frame{flow: flow, invocation: "inv:" + integerText(r.invocations), parent: parent, caller: caller, values: map[string]any{}, responses: map[string]*retainedResponse{}}
	for name, v := range inputs {
		key := domain.SourceKey(domain.Source{Source: "input", Name: name})
		if _, ok := plan.LastUses[key]; ok {
			f.values[key] = v
		}
	}
	r.frames = append(r.frames, f)
	defer func() {
		if problem != nil && f.pending {
			_ = r.emit(f, f.current, -1, "step.failed", map[string]any{"error": problem}, true)
		}
		_ = r.emit(f, -1, -1, "workflow.finished", map[string]any{"status": status(problem)}, problem != nil)
		r.frames = r.frames[:len(r.frames)-1]
	}()
	if problem = r.emit(f, -1, -1, "workflow.started", map[string]any{}, false); problem != nil {
		return nil, problem
	}
	for i, n := range w.Nodes {
		if n.Type == "start" {
			f.current = i
			break
		}
	}
	for {
		if problem = r.boundary(); problem != nil {
			return nil, problem
		}
		index := f.current
		node := w.Nodes[index]
		r.visits++
		if r.visits > 1024 {
			return nil, fail("invocationLimit", "Executed node visit limit exceeded")
		}
		if problem = r.emit(f, index, -1, "step.started", map[string]any{"kind": node.Type}, false); problem != nil {
			return nil, problem
		}
		f.pending = true
		selected := -1
		switch node.Type {
		case "http":
			problem = r.http(f, index, node)
		case "authentication":
			problem = r.authenticate(f, index, node.Config)
		case "extract":
			var body any
			body, problem = r.response(f, node.Config.SourceNodeID)
			if problem == nil {
				values := map[string]any{}
				for _, name := range orderedKeys(node.Config.ExtractEntries()) {
					entry := node.Config.ExtractEntries()[name]
					expression := plan.ExtractionValues[node.ID][name]
					value, err := expression.Evaluate(body, expressionStop(r.activeContext))
					if err != nil {
						problem = r.expressionFailure(err, false)
						break
					}
					if domain.ScalarType(value) != entry.Type {
						problem = fail("extractionWrongType", "Extracted value does not match its declared type")
						break
					}
					if value == "" {
						problem = fail("extractionEmptyValue", "Extracted string is empty")
						break
					}
					values[domain.SourceKey(domain.Source{Source: "extraction", NodeID: node.ID, Name: name})] = value
				}
				if problem == nil {
					problem = r.capacity(values)
					if problem != nil && problem.Kind == "valueLimit" {
						problem = ExtractionFailure("extractionOutputLimit")
					}
				}
				if problem == nil {
					for key, value := range values {
						r.security.rememberValue(value)
						if _, needed := plan.LastUses[key]; needed {
							f.values[key] = value
						}
					}
				}
			}
		case "setSession":
			set := map[string]string{}
			for _, name := range orderedKeys(node.Config.SetHeaders) {
				source := node.Config.SetHeaders[name]
				value, err := r.scalar(source, f, nil)
				if err != nil {
					problem = err
					break
				}
				text, ok := value.(string)
				if !ok {
					problem = fail("portTypeMismatch", "Session header source requires a string")
					break
				}
				set[name] = source.Prefix + text
			}
			if problem == nil {
				problem = r.session.UpdateHeaders(set, node.Config.RemoveHeaders)
			}
			if problem == nil {
				names := []string{}
				for _, name := range orderedKeys(set) {
					r.security.remember(set[name])
					names = append(names, r.security.text(strings.ToLower(name)))
				}
				removed := []string{}
				for _, name := range node.Config.RemoveHeaders {
					removed = append(removed, r.security.text(strings.ToLower(name)))
				}
				problem = r.emit(f, index, -1, "session.updated", map[string]any{"setHeaders": names, "removedHeaders": removed}, false)
			}
		case "call":
			binding := domain.Binding{WorkflowID: node.Config.WorkflowID, Inputs: node.Config.Inputs}
			var bound map[string]any
			bound, problem = r.binding(binding, f, nil)
			if problem == nil {
				var values map[string]any
				values, problem = r.invoke(r.flowIndex(binding.WorkflowID), bound, pointer(f.invocation), pointer(nodeKey(flow, index)))
				if problem == nil {
					for name, value := range values {
						key := domain.SourceKey(domain.Source{Source: "callOutput", NodeID: node.ID, Name: name})
						if _, needed := plan.LastUses[key]; needed {
							f.values[key] = value
						}
					}
				}
			}
		case "exclusiveSplit":
			selected, problem = r.route(f, index, node)
		}
		if problem != nil {
			return nil, problem
		}
		if problem = r.boundary(); problem != nil {
			return nil, problem
		}
		if problem = r.emit(f, index, -1, "step.finished", map[string]any{}, false); problem != nil {
			return nil, problem
		}
		f.pending = false
		for key := range f.values {
			if plan.LastUses[key] == node.ID {
				delete(f.values, key)
			}
		}
		for source := range f.responses {
			if plan.ResponseLastUses[source] == node.ID {
				delete(f.responses, source)
			}
		}
		if node.Type == "end" {
			break
		}
		if selected < 0 {
			for ei, edge := range w.Edges {
				if edge.From == node.ID {
					selected = ei
					break
				}
			}
		}
		if selected < 0 {
			return nil, fail("invalidProgram", "No outgoing edge is available")
		}
		if problem = r.emit(f, -1, selected, "edge.traversed", map[string]any{}, false); problem != nil {
			return nil, problem
		}
		target := w.Edges[selected].To
		for next, n := range w.Nodes {
			if n.ID == target {
				f.current = next
				break
			}
		}
	}
	outputs = map[string]any{}
	for _, name := range orderedKeys(w.Outputs) {
		output := w.Outputs[name]
		value, err := r.scalar(output.Value, f, nil)
		if err != nil {
			return nil, err
		}
		if domain.ScalarType(value) != output.Type {
			return nil, fail("portTypeMismatch", "Workflow output has an invalid scalar type or size")
		}
		outputs[name] = value
		if text, ok := value.(string); ok {
			r.security.remember(text)
		}
	}
	if problem = r.capacity(outputs); problem != nil {
		return nil, problem
	}
	return outputs, nil
}
func (r *runner) route(f *frame, index int, node domain.Node) (int, *Failure) {
	w := r.program.Document.Workflows[f.flow]
	plan := r.program.Flows[f.flow]
	source := node.Config.SourceNodeID
	statusValue := f.values[domain.SourceKey(domain.Source{Source: "response", NodeID: source})]
	code, ok := domain.Integer(statusValue)
	if !ok {
		return -1, fail("routeSourceUnavailable", "Route source status is unavailable")
	}
	var body any
	var problem *Failure
	for ei, e := range w.Edges {
		if e.From == node.ID && plan.Predicates[ei] != nil && plan.Predicates[ei].RequiresBody() {
			body, problem = r.response(f, source)
			if problem != nil {
				return -1, problem
			}
			break
		}
	}
	inputs := map[string]any{}
	for _, name := range orderedKeys(node.Config.Inputs) {
		value, err := r.scalar(node.Config.Inputs[name], f, nil)
		if err != nil {
			return -1, err
		}
		inputs[name] = value
	}
	if problem = r.capacity(inputs); problem != nil {
		return -1, problem
	}
	matches := []int{}
	fallback := -1
	outgoing := []int{}
	for ei, e := range w.Edges {
		if e.From != node.ID {
			continue
		}
		outgoing = append(outgoing, ei)
		if e.Default != nil && *e.Default {
			fallback = ei
			continue
		}
		predicate := plan.Predicates[ei]
		value, err := predicate.Evaluate(uint16(code), body, inputs, expressionStop(r.activeContext))
		if err != nil {
			return -1, r.expressionFailure(err, true)
		}
		if value {
			matches = append(matches, ei)
		}
	}
	if len(matches) > 1 {
		return -1, fail("routeAmbiguous", "More than one route condition matched")
	}
	selected := fallback
	if len(matches) == 1 {
		selected = matches[0]
	}
	if selected < 0 {
		return -1, fail("routeNoMatch", "No route matched and no default exists")
	}
	unselected := []string{}
	for _, e := range outgoing {
		if e != selected {
			unselected = append(unselected, edgeKey(f.flow, e))
		}
	}
	problem = r.emit(f, index, -1, "route.selected", map[string]any{"selectedEdgeKey": edgeKey(f.flow, selected), "unselectedEdgeKeys": unselected}, false)
	return selected, problem
}
func (r *runner) send(f *frame, index int, request Request) (Response, *Failure) {
	r.security.url(request.URL)
	headers := map[string][]string{}
	for k, v := range request.Headers {
		headers[k] = []string{v}
	}
	var preview any
	parseFailure := fail("notDecoded", "Preview unavailable")
	if !r.security.conservative && request.JSONBody {
		preview, parseFailure = r.workspace.runs.JSON.Parse(r.activeContext, request.Body)
	}
	if problem := r.emit(f, index, -1, "request.sent", map[string]any{"method": request.Method, "url": r.security.url(request.URL), "headers": r.security.headers(headers), "body": r.security.body(preview, parseFailure == nil, len(request.Body), false, r.options.MaxPreviewBytes)}, false); problem != nil {
		return Response{}, problem
	}
	if problem := r.boundary(); problem != nil {
		return Response{}, problem
	}
	r.report.Summary.Requests++
	response, problem := r.session.Do(r.activeContext, request)
	if problem != nil {
		if boundary := r.boundary(); boundary != nil {
			return Response{}, boundary
		}
		return Response{}, problem
	}
	r.report.Summary.Responses++
	if response.Status >= 400 {
		r.report.Summary.HTTPErrorResponses++
	}
	preview = nil
	parseFailure = fail("notDecoded", "Preview unavailable")
	if !r.security.conservative && !response.Truncated {
		preview, parseFailure = r.workspace.runs.JSON.Parse(r.activeContext, response.Body)
		r.security.learn(preview)
	}
	if problem = r.emit(f, index, -1, "response.received", map[string]any{"status": response.Status, "headers": r.security.headers(response.Headers), "body": r.security.body(preview, parseFailure == nil, len(response.Body), response.Truncated, r.options.MaxPreviewBytes)}, false); problem != nil {
		return Response{}, problem
	}
	return response, nil
}
func (r *runner) http(f *frame, index int, node domain.Node) *Failure {
	input, problem := r.httpInput(node.Config, f)
	if problem != nil {
		return problem
	}
	request, problem := r.session.Prepare(input)
	if problem != nil {
		return problem
	}
	response, problem := r.send(f, index, request)
	if problem != nil {
		return problem
	}
	plan := r.program.Flows[f.flow]
	key := domain.SourceKey(domain.Source{Source: "response", NodeID: node.ID})
	if _, needed := plan.LastUses[key]; needed {
		f.values[key] = int64(response.Status)
	}
	if _, needed := plan.ResponseLastUses[node.ID]; needed {
		f.responses[node.ID] = &retainedResponse{response: response}
	}
	return r.capacity(nil)
}
func (r *runner) authenticate(f *frame, index int, config domain.Config) (problem *Failure) {
	if problem = r.session.Reset(); problem != nil {
		return problem
	}
	defer func() {
		if problem != nil {
			_ = r.session.Reset()
		}
	}()
	key := authKey(f.flow, index)
	username, uok := r.credentials[key+"\x00username"]
	password, pok := r.credentials[key+"\x00password"]
	if !uok || !pok {
		return fail("authenticationCredentials", "Authentication credentials unavailable")
	}
	input, problem := r.httpInput(config, nil)
	if problem != nil {
		return problem
	}
	input.Body, _ = jsonWire(map[string]string{config.UsernameKey: username, config.PasswordKey: password})
	input.ContentType = "application/json"
	input.JSONBody = true
	request, problem := r.session.Prepare(input)
	if problem != nil {
		return problem
	}
	response, problem := r.send(f, index, request)
	if problem != nil {
		return problem
	}
	if response.Status < 200 || response.Status >= 300 {
		return fail("authenticationFailed", "Authentication requires a successful HTTP response")
	}
	if config.Mode == "cookie" {
		if !r.session.HasResponseCookies(request.URL, response) {
			return fail("authenticationFailed", "Authentication response did not establish a session cookie")
		}
	} else {
		if response.Truncated {
			return fail("extractionResponseTruncated", "Authentication response was truncated")
		}
		body, err := r.workspace.runs.JSON.Parse(r.activeContext, response.Body)
		if err != nil {
			if boundary := r.boundary(); boundary != nil {
				return boundary
			}
			return err
		}
		expression := r.program.Flows[f.flow].AuthTokens[r.program.Document.Workflows[f.flow].Nodes[index].ID]
		value, errValue := expression.Evaluate(body, expressionStop(r.activeContext))
		if errValue != nil {
			if errValue.Error() == "nullValue" {
				return ExtractionFailure("extractionWrongType")
			}
			return r.expressionFailure(errValue, false)
		}
		token, ok := value.(string)
		if !ok {
			return fail("extractionWrongType", "Authentication token must be a string")
		}
		if token == "" {
			return fail("extractionEmptyValue", "Authentication token is empty")
		}
		r.security.remember(token)
		if problem = r.session.UpdateHeaders(map[string]string{config.HeaderName: config.HeaderPrefix + token}, nil); problem != nil {
			return problem
		}
	}
	return r.boundary()
}
