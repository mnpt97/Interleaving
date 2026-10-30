package domain

// visitSources traverses descriptor nodes only. Literal body payloads and literal
// template subtrees remain opaque JSON and never acquire reference semantics.
func visitSources(n Node, visit func(Source)) {
	switch n.Type {
	case "exclusiveSplit":
		visit(Source{Source: "response", NodeID: n.Config.SourceNodeID, Field: "status"})
		fallthrough
	case "call":
		for _, k := range keys(n.Config.Inputs) {
			visit(n.Config.Inputs[k])
		}
	case "setSession":
		for _, k := range keys(n.Config.SetHeaders) {
			visit(n.Config.SetHeaders[k])
		}
	case "http":
		if n.Config.Body != nil && n.Config.Body.Kind == "jsonTemplate" {
			var walk func(Source)
			walk = func(s Source) {
				switch s.Source {
				case "literal":
				case "object":
					for _, k := range keys(s.Fields) {
						walk(s.Fields[k])
					}
				case "array":
					for _, s := range s.Items {
						walk(s)
					}
				default:
					visit(s)
				}
			}
			for _, k := range keys(n.Config.Body.Fields) {
				walk(n.Config.Body.Fields[k])
			}
		}
	}
}

// valueKey is unambiguous even when user-authored IDs contain punctuation.
func valueKey(s Source) string {
	switch s.Source {
	case "input":
		return "input:" + Pointer("", s.Name)
	case "response":
		return "response:" + Pointer("", s.NodeID)
	case "extraction", "callOutput":
		return s.Source + ":" + Pointer(Pointer("", s.NodeID), s.Name)
	}
	return ""
}

// SourceKey identifies invocation-local storage shared by compiler and runner.
func SourceKey(s Source) string { return valueKey(s) }
func (c *compiler) workflowByID(id string) *Workflow {
	for i := range c.doc.Workflows {
		if c.doc.Workflows[i].ID == id {
			return &c.doc.Workflows[i]
		}
	}
	return nil
}
func (c *compiler) sourceType(s Source, w *Workflow, g *graph, consumer, path string, init *Workflow) string {
	local := s.Source == "extraction" || s.Source == "callOutput" || s.Source == "response"
	if local && w != nil {
		if !g.dominates(s.NodeID, consumer) {
			c.add("valueNotAvailable", path, "Source must be available on every path reaching its consumer.")
			return ""
		}
	}
	kind := ""
	switch s.Source {
	case "literal":
		kind = ScalarType(s.Value)
	case "environment":
		kind = ScalarType(c.doc.Environment.Variables[s.Name])
	case "identity":
		c.id(s.Field, path)
		c.identityFields[s.Field] = true
		kind = "string"
	case "input":
		if w != nil {
			kind = w.Inputs[s.Name].Type
		}
	case "lifecycle":
		if init != nil {
			kind = init.Outputs[s.Name].Type
		}
	case "extraction", "callOutput", "response":
		if w == nil {
			c.add("valueNotAvailable", path, "Source must precede its consumer in this invocation.")
			return ""
		}
		index, ok := g.indices[s.NodeID]
		if !ok {
			c.add("valueNotAvailable", path, "Source must precede its consumer in this invocation.")
			return ""
		}
		node := w.Nodes[index]
		switch {
		case s.Source == "extraction" && node.Type == "extract":
			kind = node.Config.ExtractEntries()[s.Name].Type
		case s.Source == "callOutput" && node.Type == "call":
			if callee := c.workflowByID(node.Config.WorkflowID); callee != nil {
				kind = callee.Outputs[s.Name].Type
			}
		case s.Source == "response" && node.Type == "http":
			kind = "integer"
		}
	}
	if kind == "" {
		c.add("valueNotAvailable", path, "Scalar source is missing, oversized, mistyped, or outside its allowed scope.")
	}
	return kind
}
func (c *compiler) binding(b Binding, w *Workflow, g *graph, consumer, path string, init *Workflow) {
	callee := c.workflowByID(b.WorkflowID)
	if callee == nil {
		c.add("missingWorkflow", path, "Referenced workflow does not exist.")
		return
	}
	for _, name := range keys(callee.Inputs) {
		port := callee.Inputs[name]
		if src, ok := b.Inputs[name]; ok {
			kind := c.sourceType(src, w, g, consumer, path, init)
			if kind != "" && kind != port.Type {
				c.add("portTypeMismatch", path, "Input type does not match its declaration.")
			}
		} else if port.Default == nil {
			c.add("missingInput", path, "Required workflow input is missing.")
		}
	}
	for _, name := range keys(b.Inputs) {
		if _, ok := callee.Inputs[name]; !ok {
			c.add("unknownInput", path, "Mapping names an undeclared input.")
		}
	}
	if w == nil {
		for _, name := range keys(b.Inputs) {
			switch b.Inputs[name].Source {
			case "literal", "environment", "identity", "lifecycle":
			default:
				c.add("invalidLifecycleReference", path, "Lifecycle inputs cannot access invocation-local values.")
			}
		}
	}
}
