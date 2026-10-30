package application

import (
	"fmt"
	"interleaving/internal/domain"
	"strings"
)

func (s *security) snapshot(doc *domain.Document, targets map[string]string) any {
	root, err := admittedWire(doc)
	if err != nil {
		return "[Preview omitted: serialization failed]"
	}
	var value any = root
	// Profile URLs are normalized in the public wire model; per-node overrides
	// remain authored strings. Preserve that distinction in the public snapshot.
	if root, ok := value.(map[string]any); ok {
		profiles, _ := root["clientProfiles"].([]any)
		for _, raw := range profiles {
			if profile, ok := raw.(map[string]any); ok {
				if text, ok := profile["baseUrl"].(string); ok {
					if target, exists := targets[text]; exists {
						profile["baseUrl"] = target
					}
				}
			}
		}
	}
	var scrub func(any)
	scrub = func(v any) {
		switch v := v.(type) {
		case map[string]any:
			delete(v, "layout")
			if v["type"] == "extract" {
				if config, ok := v["config"].(map[string]any); ok {
					if config["type"] == "string" {
						delete(config, "type")
					}
					if outputs, ok := config["outputs"].(map[string]any); ok {
						for _, raw := range outputs {
							if output, ok := raw.(map[string]any); ok && output["type"] == "string" {
								delete(output, "type")
							}
						}
					}
				}
			}
			if condition, ok := v["condition"].(map[string]any); ok {
				condition["expression"] = "[Expression omitted]"
			}
			if v["source"] == "literal" {
				v["value"] = "[REDACTED]"
				return
			}
			if v["kind"] == "json" || v["kind"] == "text" {
				v["value"] = "[Payload omitted]"
			}
			if h, ok := v["headers"].(map[string]any); ok {
				for k := range h {
					h[k] = "[REDACTED]"
				}
			}
			for k, child := range v {
				if k == "prefix" || k == "headerPrefix" || k == "default" {
					v[k] = "[REDACTED]"
				} else if k == "baseUrl" {
					if text, ok := child.(string); ok {
						v[k] = s.url(text)
					}
				} else if k == "path" {
					if text, ok := child.(string); ok {
						v[k] = s.text(strings.SplitN(text, "?", 2)[0])
					}
				} else {
					scrub(child)
				}
			}
		case []any:
			for _, v := range v {
				scrub(v)
			}
		}
	}
	scrub(value)
	if root, ok := value.(map[string]any); ok {
		if env, ok := root["environment"].(map[string]any); ok {
			if vars, ok := env["variables"].(map[string]any); ok {
				for k := range vars {
					vars[k] = "[REDACTED]"
				}
			}
		}
	}
	var references func(any) any
	references = func(v any) any {
		switch v := v.(type) {
		case string:
			return s.text(v)
		case map[string]any:
			r := map[string]any{}
			for k, v := range v {
				r[s.text(k)] = references(v)
			}
			return r
		case []any:
			r := []any{}
			for _, v := range v {
				r = append(r, references(v))
			}
			return r
		default:
			return v
		}
	}
	return references(value)
}
func label(s *security, value string) string {
	r := []rune(s.text(value))
	if len(r) > 256 {
		r = r[:256]
	}
	return string(r)
}
func graphProjection(program *domain.Program, s *security) any {
	doc := program.Document
	flows := []any{}
	for wi, w := range doc.Workflows {
		nodes := []any{}
		indices := map[string]int{}
		for ni, n := range w.Nodes {
			indices[n.ID] = ni
			position := domain.Position{X: 80 + float64(ni)*260, Y: 160}
			if w.Layout != nil {
				if p, ok := w.Layout.Positions[n.ID]; ok {
					position = p
				}
			}
			name := n.ID
			if n.Name != nil {
				name = *n.Name
			}
			nodes = append(nodes, map[string]any{"key": nodeKey(wi, ni), "id": label(s, n.ID), "name": label(s, name), "type": n.Type, "position": position})
		}
		edges := []any{}
		for ei, e := range w.Edges {
			connection := domain.Connection{Source: "right", Target: "left"}
			if w.Layout != nil {
				if c, ok := w.Layout.Connections[e.ID]; ok {
					connection = c
				}
			}
			edges = append(edges, map[string]any{"key": edgeKey(wi, ei), "id": label(s, e.ID), "fromNodeKey": nodeKey(wi, indices[e.From]), "toNodeKey": nodeKey(wi, indices[e.To]), "sourceHandle": connection.Source, "targetHandle": connection.Target})
		}
		flows = append(flows, map[string]any{"key": fmt.Sprintf("wf:%d", wi), "id": label(s, w.ID), "name": label(s, w.Name), "nodes": nodes, "edges": edges})
	}
	l := doc.Groups[0].Lifecycle
	binding := func(b *domain.Binding) any {
		if b == nil {
			return nil
		}
		for i, w := range doc.Workflows {
			if w.ID == b.WorkflowID {
				return fmt.Sprintf("wf:%d", i)
			}
		}
		return nil
	}
	inline := []string{}
	for _, p := range []struct {
		name string
		b    *domain.Binding
	}{{"initialize", l.Initialize}, {"iteration", &l.Iteration}, {"cleanup", l.Cleanup}} {
		if p.b == nil {
			continue
		}
		uses := 0
		for _, b := range []*domain.Binding{l.Initialize, &l.Iteration, l.Cleanup} {
			if b != nil && b.WorkflowID == p.b.WorkflowID {
				uses++
			}
		}
		called := false
		for _, w := range doc.Workflows {
			for _, n := range w.Nodes {
				if n.Type == "call" && n.Config.WorkflowID == p.b.WorkflowID {
					called = true
				}
			}
		}
		if uses == 1 && !called {
			inline = append(inline, p.name)
		}
	}
	return map[string]any{"groups": []any{map[string]any{"key": "group:0", "id": label(s, doc.Groups[0].ID), "initialize": binding(l.Initialize), "iteration": binding(&l.Iteration), "cleanup": binding(l.Cleanup), "inlinePhases": inline}}, "workflows": flows}
}
func nodeKey(w, n int) string      { return fmt.Sprintf("wf:%d/node:%d", w, n) }
func edgeKey(w, e int) string      { return fmt.Sprintf("wf:%d/edge:%d", w, e) }
func pointer(value string) *string { return &value }
