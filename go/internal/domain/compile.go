package domain

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

// Program is a validation result for later execution phases. Compilation owns no
// HTTP client, environment lookup, logger implementation, file, or desktop SDK.
type Program struct {
	Document        *Document
	Flows           []FlowPlan
	Reachable       []int
	RequiredSecrets []RequiredSecret
	Targets         map[string]string
}
type RequiredSecret struct {
	IdentityID, Field string
	Reference         SecretReference
}
type compiler struct {
	checks
	doc               *Document
	expressions       Expressions
	graphs            []*graph
	flows             []FlowPlan
	identityFields    map[string]bool
	targetURLs        TargetURLs
	normalizedTargets map[string]string
}

func keys[V any](m map[string]V) []string {
	r := make([]string, 0, len(m))
	for k := range m {
		r = append(r, k)
	}
	sort.Strings(r)
	return r
}
func (c *compiler) validBaseURL(s string) bool {
	normalized, valid := c.targetURLs.BaseURL(s)
	if valid {
		c.normalizedTargets[s] = normalized
	}
	return valid
}
func expressionLocation(d *Diagnostic, source string, offset int) {
	line, column := 1, 1
	for i, ch := range source {
		if i >= offset {
			break
		}
		if ch == '\n' {
			line++
			column = 1
		} else {
			column++
		}
	}
	d.ExpressionLine = &line
	d.ExpressionColumn = &column
}
func (c *checks) expression(e *ExpressionError, path, source, category, node string) {
	if e == nil {
		return
	}
	before := len(c.errors)
	c.problem(category, e.Code, path, "Expression is outside the supported bounded FEEL subset.", node)
	if len(c.errors) > before {
		expressionLocation(&c.errors[len(c.errors)-1], source, e.Offset)
	}
}

// Compile checks every definition (including unused ones), then derives bounded
// lifecycle reachability and credential descriptors. Cancellation is observed
// between compiler passes; the admitted document is at most 256 nodes.
func Compile(ctx context.Context, doc *Document, expressions Expressions, targets TargetURLs) (*Program, Diagnostics) {
	c := compiler{doc: doc, expressions: expressions, identityFields: map[string]bool{}, targetURLs: targets, normalizedTargets: map[string]string{}}
	if err := ctx.Err(); err != nil {
		return nil, Diagnostics{Problem("validation", "cancelled", "", "Validation cancelled.")}
	}
	// Reject composition budgets before visiting leaves or copying template data.
	for _, w := range doc.Workflows {
		for _, n := range w.Nodes {
			if n.Type == "http" && n.Config.Body != nil && n.Config.Body.Kind == "jsonTemplate" {
				count := 1
				var visit func(Source, int) bool
				visit = func(s Source, depth int) bool {
					count++
					if count > 256 || depth > 32 {
						return false
					}
					if s.Source == "object" {
						for _, k := range keys(s.Fields) {
							if !visit(s.Fields[k], depth+1) {
								return false
							}
						}
					}
					if s.Source == "array" {
						for _, s := range s.Items {
							if !visit(s, depth+1) {
								return false
							}
						}
					}
					return true
				}
				for _, k := range keys(n.Config.Body.Fields) {
					if !visit(n.Config.Body.Fields[k], 2) {
						return nil, Diagnostics{Problem("validation", "templateLimit", "/workflows", "Template composition exceeds depth or node limits.")}
					}
				}
			}
		}
	}
	if doc.SchemaVersion != 2 {
		c.add("unsupportedVersion", "/schemaVersion", "Expected schemaVersion 2.")
	}
	if len(doc.Groups) != 1 {
		c.add("runConfiguration", "/groups", "Execution requires exactly one once workload group.")
		return nil, c.errors
	}
	nodes, edges := 0, 0
	for _, w := range doc.Workflows {
		nodes += len(w.Nodes)
		edges += len(w.Edges)
	}
	if len(doc.Workflows) == 0 || len(doc.Workflows) > 32 || nodes > 256 || edges > 1024 {
		c.add("resourceLimit", "/workflows", "Workflow, node, or edge limits exceeded.")
		return nil, c.errors
	}
	c.id(doc.ID, "/id")
	c.id(doc.Groups[0].ID, "/groups/0/id")
	for _, p := range doc.ClientProfiles {
		c.id(p.ID, "/clientProfiles")
	}
	if len(doc.Name) > 256 {
		c.add("resourceLimit", "/name", "Display names allow at most 256 UTF-8 bytes.")
	}
	ids := map[string]bool{}
	predicateBytes := 0
	for wi := range doc.Workflows {
		if ctx.Err() != nil {
			return nil, Diagnostics{Problem("validation", "cancelled", "", "Validation cancelled.")}
		}
		w := &doc.Workflows[wi]
		path := fmt.Sprintf("/workflows/%d", wi)
		start := len(c.errors)
		c.workflow = nil
		c.id(w.ID, path)
		if ids[w.ID] {
			c.add("duplicateWorkflowId", path, "Workflow IDs must be unique.")
		}
		ids[w.ID] = true
		if len(w.Name) > 256 || len(w.Inputs) > 32 || len(w.Outputs) > 32 {
			c.add("resourceLimit", path, "Workflow name or port count exceeds limits.")
		}
		for _, name := range keys(w.Inputs) {
			p := w.Inputs[name]
			c.identifier(name, path)
			if p.Default != nil {
				dec := json.NewDecoder(strings.NewReader(string(p.Default)))
				dec.UseNumber()
				var value any
				_ = dec.Decode(&value)
				if ScalarType(value) != p.Type {
					c.add("portTypeMismatch", path, "Input default must match its scalar type and bounds.")
				}
			}
		}
		for _, name := range keys(w.Outputs) {
			c.identifier(name, path)
		}
		edgeIDs := map[string]bool{}
		for _, e := range w.Edges {
			c.id(e.ID, path)
			if edgeIDs[e.ID] {
				c.add("duplicateEdgeId", path, "Edge IDs must be unique within a workflow.")
			}
			edgeIDs[e.ID] = true
		}
		for ni, n := range w.Nodes {
			c.id(n.ID, path)
			p := fmt.Sprintf("%s/nodes/%d/config", path, ni)
			if n.Config.BaseURL != nil && (n.Type == "http" || n.Type == "authentication") && !c.validBaseURL(*n.Config.BaseURL) {
				c.add("invalidBaseUrl", p+"/baseUrl", "Use an absolute HTTP(S) base URL without credentials, query or fragment.")
			}
			if n.Type == "authentication" {
				v := n.Config
				if !(v.Method == "POST" || v.Method == "PUT" || v.Method == "PATCH") || !strings.HasPrefix(v.Path, "/") || strings.HasPrefix(v.Path, "//") || strings.ContainsAny(v.Path, "\r\n#") {
					c.add("invalidAuthenticationRequest", p, "Use POST, PUT or PATCH and a path starting with one slash.")
				}
				c.identifier(v.Username.Name, p)
				c.identifier(v.Password.Name, p)
				c.identifier(v.UsernameKey, p)
				c.identifier(v.PasswordKey, p)
				if v.UsernameKey == v.PasswordKey {
					c.add("invalidCredentialFields", p, "Username and password JSON fields must differ.")
				}
				if v.Mode == "bearer" {
					if len(v.HeaderPrefix) > 8192 || strings.ContainsAny(v.HeaderPrefix, "\r\n") || len(v.HeaderName) > 128 || !HeaderName(v.HeaderName) {
						c.add("invalidHeader", p, "Use a valid session header and a prefix without line breaks.")
					}
					if _, e := expressions.CompileExtraction(v.TokenExpression); e != nil {
						c.add(e.Code, p, "Unsupported authentication token expression.")
					}
				}
			}
			if n.Name != nil && len(*n.Name) > 256 {
				c.add("resourceLimit", fmt.Sprintf("%s/nodes/%d/name", path, ni), "Display names allow at most 256 UTF-8 bytes.")
			}
		}
		local := checks{}
		g := local.workflowGraph(doc, w, path, c.validBaseURL)
		if g != nil {
			c.validateAuth(&local, w, g, path)
		}
		if len(local.errors) > 0 {
			for _, d := range local.errors {
				d.WorkflowID = &w.ID
				if len(c.errors) < 100 {
					c.errors = append(c.errors, d)
				}
			}
			c.graphs = append(c.graphs, nil)
			c.flows = append(c.flows, FlowPlan{})
			continue
		}
		c.workflow = &w.ID
		flow := FlowPlan{Order: append([]int(nil), g.order...), Pairs: g.pairs, Dominators: g.dom, Predicates: map[int]Predicate{}, Extractions: map[string]map[string][]string{}, ExtractionValues: map[string]map[string]ScalarExpression{}, AuthTokens: map[string]ScalarExpression{}, LastUses: map[string]string{}, SensitiveHTTP: map[string]bool{}, ResponseLastUses: map[string]string{}}
		for ei, e := range w.Edges {
			if e.Condition == nil {
				continue
			}
			p := fmt.Sprintf("%s/edges/%d/condition/expression", path, ei)
			predicate, err := expressions.CompilePredicate(e.Condition.Expression)
			if err != nil {
				c.expression(err, p, e.Condition.Expression, "validation", "")
				continue
			}
			split := w.Nodes[g.indices[e.From]]
			for _, name := range predicate.InputNames() {
				if _, ok := split.Config.Inputs[name]; !ok {
					c.add("missingRouteInput", p, "Bind every referenced route input on the split.")
				}
			}
			if predicate.RequiresBody() {
				flow.SensitiveHTTP[split.Config.SourceNodeID] = true
				flow.ResponseLastUses[split.Config.SourceNodeID] = e.From
			}
			predicateBytes += len(e.Condition.Expression) + predicate.StorageBytes()
			if predicateBytes > 256*1024 {
				c.add("expressionLimit", p, "Compiled route predicates exceed the document budget.")
			} else {
				flow.Predicates[ei] = predicate
			}
		}
		for _, index := range g.order {
			n := w.Nodes[index]
			v := n.Config
			if n.Type == "exclusiveSplit" {
				if len(v.Inputs) > 32 {
					c.add("resourceLimit", path, "Gateways allow at most 32 scalar input bindings.")
				}
				types := map[string]string{}
				for _, name := range keys(v.Inputs) {
					c.identifier(name, path)
					types[name] = c.sourceType(v.Inputs[name], w, g, n.ID, path, nil)
				}
				for ei, e := range w.Edges {
					if e.From == n.ID && flow.Predicates[ei] != nil {
						if err := flow.Predicates[ei].ValidateInputTypes(types); err != nil {
							c.add("invalidRouteCondition", fmt.Sprintf("%s/edges/%d/condition/expression", path, ei), "Route operands have incompatible scalar types.")
						}
					}
				}
			}
			if n.Type == "call" {
				c.binding(Binding{WorkflowID: v.WorkflowID, Inputs: v.Inputs}, w, g, n.ID, path, nil)
			}
			visitSources(n, func(s Source) {
				kind := c.sourceType(s, w, g, n.ID, path, nil)
				if n.Type == "setSession" && kind != "" && kind != "string" {
					c.add("portTypeMismatch", path, "Session headers require string sources.")
				}
				if s.Source == "lifecycle" {
					c.add("invalidLifecycleReference", path, "Lifecycle references occur only in lifecycle bindings.")
				}
				if key := valueKey(s); key != "" {
					flow.LastUses[key] = n.ID
				}
			})
			if n.Type == "extract" {
				outputs := map[string][]string{}
				values := map[string]ScalarExpression{}
				for _, name := range keys(v.ExtractEntries()) {
					out := v.ExtractEntries()[name]
					p, _ := expressions.CompileExtraction(out.Expression)
					outputs[name] = p
					expression, err := expressions.CompileExtractionValue(out.Expression)
					if err != nil {
						c.expression(err, path, out.Expression, "validation", n.ID)
					} else {
						values[name] = expression
					}
				}
				flow.Extractions[n.ID] = outputs
				flow.ExtractionValues[n.ID] = values
				flow.ResponseLastUses[v.SourceNodeID] = n.ID
				flow.SensitiveHTTP[v.SourceNodeID] = true
			}
			if n.Type == "authentication" && v.Mode == "bearer" {
				expression, err := expressions.CompileExtractionValue(v.TokenExpression)
				if err != nil {
					c.expression(err, path, v.TokenExpression, "validation", n.ID)
				} else {
					flow.AuthTokens[n.ID] = expression
				}
			}
		}
		end := ""
		for _, n := range w.Nodes {
			if n.Type == "end" {
				end = n.ID
			}
		}
		returnMarker := "$return"
		for {
			if _, ok := g.indices[returnMarker]; !ok {
				break
			}
			returnMarker += "_"
		}
		for _, name := range keys(w.Outputs) {
			out := w.Outputs[name]
			kind := c.sourceType(out.Value, w, g, end, path, nil)
			if kind != "" && kind != out.Type {
				c.add("portTypeMismatch", path, "Output source type differs from declaration.")
			}
			if key := valueKey(out.Value); key != "" {
				flow.LastUses[key] = returnMarker
			}
		}
		if len(g.pairs) > 0 {
			for key := range flow.LastUses {
				flow.LastUses[key] = returnMarker
			}
			for key := range flow.ResponseLastUses {
				flow.ResponseLastUses[key] = returnMarker
			}
		}
		sensitive := false
		for _, index := range g.order {
			n := w.Nodes[index]
			if n.Type == "authentication" || n.Type == "setSession" && len(n.Config.SetHeaders) > 0 {
				sensitive = true
			}
			if n.Type == "http" {
				visitSources(n, func(Source) { sensitive = true })
				if flow.SensitiveHTTP[n.ID] {
					sensitive = true
				}
				if sensitive {
					flow.SensitiveHTTP[n.ID] = true
				}
			}
		}
		for i := start; i < len(c.errors); i++ {
			c.errors[i].WorkflowID = &w.ID
		}
		c.graphs = append(c.graphs, g)
		c.flows = append(c.flows, flow)
	}
	c.workflow = nil
	l := doc.Groups[0].Lifecycle
	if l.CleanupTimeoutMS < 1 || l.CleanupTimeoutMS > 5000 {
		c.add("invalidTimeout", "/groups/0/lifecycle/cleanupTimeoutMs", "Cleanup timeout must be between 1 and 5000 ms.")
	}
	var init *Workflow
	if l.Initialize != nil {
		init = c.workflowByID(l.Initialize.WorkflowID)
	}
	for _, phase := range []struct {
		name string
		b    *Binding
	}{{"initialize", l.Initialize}, {"iteration", &l.Iteration}, {"cleanup", l.Cleanup}} {
		if phase.b != nil {
			var allowed *Workflow
			if phase.name == "iteration" {
				allowed = init
			}
			c.binding(*phase.b, nil, nil, "", "/groups/0/lifecycle/"+phase.name, allowed)
		}
	}
	if len(c.errors) > 0 {
		return nil, c.errors
	}
	reachable := c.reachable(l)
	if len(c.errors) > 0 {
		return nil, c.errors
	}
	http := false
	for _, i := range reachable {
		for _, n := range doc.Workflows[i].Nodes {
			if n.Type == "http" || n.Type == "authentication" {
				http = true
			}
		}
	}
	if !http {
		c.add("missingHttp", "/groups", "A runnable lifecycle requires at least one reachable HTTP node.")
		return nil, c.errors
	}
	secrets := c.secrets(reachable)
	if len(c.errors) > 0 {
		return nil, c.errors
	}
	// A plan owns its document snapshot; later editor/caller mutations cannot alter
	// checked execution inputs. UseNumber preserves exact admitted numeric values.
	data, err := json.Marshal(doc)
	if err != nil {
		return nil, Diagnostics{Problem("schema", "serialization", "", "Scenario cannot be captured for compilation.")}
	}
	var snapshot Document
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.UseNumber()
	if err := decoder.Decode(&snapshot); err != nil {
		return nil, Diagnostics{Problem("schema", "serialization", "", "Scenario cannot be captured for compilation.")}
	}
	snapshot.SourceJSON = append([]byte(nil), doc.SourceJSON...)
	return &Program{Document: &snapshot, Flows: c.flows, Reachable: reachable, RequiredSecrets: secrets, Targets: c.normalizedTargets}, nil
}

func (c *checks) workflowGraph(doc *Document, w *Workflow, path string, validBaseURL func(string) bool) *graph {
	if strings.TrimSpace(doc.ID) == "" {
		c.add("emptyId", "/id", "Scenario ID must not be empty.")
	}
	if strings.TrimSpace(doc.Name) == "" {
		c.add("emptyName", "/name", "Scenario name must not be empty.")
	}
	ids := []string{}
	profiles := map[string]bool{}
	for _, p := range doc.ClientProfiles {
		ids = append(ids, p.ID)
		profiles[p.ID] = true
	}
	c.ids(ids, "/clientProfiles")
	groupIDs := []string{}
	for _, g := range doc.Groups {
		groupIDs = append(groupIDs, g.ID)
	}
	c.ids(groupIDs, "/groups")
	if len(doc.ClientProfiles) == 0 {
		c.add("missingProfile", "/clientProfiles", "Declare at least one client profile.")
	}
	for i, p := range doc.ClientProfiles {
		path := fmt.Sprintf("/clientProfiles/%d", i)
		if !validBaseURL(p.BaseURL) {
			c.add("invalidBaseUrl", path+"/baseUrl", "Use an absolute HTTP(S) URL without embedded credentials.")
		}
		if p.TimeoutMS == 0 {
			c.add("invalidTimeout", path+"/timeoutMs", "Timeout must be greater than zero.")
		}
		c.headers(p.Headers, path+"/headers", "")
	}
	for i, g := range doc.Groups {
		if !profiles[g.ClientProfileID] {
			c.add("missingProfile", fmt.Sprintf("/groups/%d/clientProfileId", i), "Referenced client profile does not exist.")
		}
	}
	for i, n := range w.Nodes {
		if n.Type == "http" {
			p := fmt.Sprintf("%s/nodes/%d/config", path, i)
			if !strings.HasPrefix(n.Config.Path, "/") || strings.HasPrefix(n.Config.Path, "//") || strings.ContainsAny(n.Config.Path, "\r\n") {
				c.add("invalidRequestPath", p+"/path", "Use a relative request path starting with a single slash.", n.ID)
			}
			if n.Config.TimeoutMS != nil && *n.Config.TimeoutMS == 0 {
				c.add("invalidTimeout", p+"/timeoutMs", "Timeout must be greater than zero.", n.ID)
			}
			c.headers(n.Config.Headers, p+"/headers", n.ID)
		}
	}
	g := c.validateGraph(w, path)
	if w.Layout != nil {
		edgeIDs := map[string]bool{}
		for _, e := range w.Edges {
			edgeIDs[e.ID] = true
		}
		for _, id := range keys(w.Layout.Connections) {
			if !edgeIDs[id] {
				c.add("unknownLayoutEdge", Pointer(path+"/layout/connections", id), "Connection layout refers to a missing edge.")
			}
		}
		for _, id := range keys(w.Layout.Positions) {
			found := false
			for _, n := range w.Nodes {
				if n.ID == id {
					found = true
				}
			}
			if !found {
				c.add("unknownLayoutNode", Pointer(path+"/layout/positions", id), "Position refers to a missing workflow node.", id)
			}
		}
	}
	if len(c.errors) > 0 {
		return nil
	}
	return g
}
