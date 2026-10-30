package application

import (
	"bytes"
	"encoding/json"
	"interleaving/internal/domain"
)

// admittedWire applies the reference document's omitted-default rules only at
// known schema locations. Literal/template payload objects remain opaque.
func admittedWire(doc *domain.Document) (map[string]any, error) {
	source := doc.SourceJSON
	if len(source) == 0 {
		source, _ = json.Marshal(doc)
	}
	var root map[string]any
	decoder := json.NewDecoder(bytes.NewReader(source))
	decoder.UseNumber()
	if err := decoder.Decode(&root); err != nil {
		return nil, err
	}
	empty := func(m map[string]any, key string) {
		switch value := m[key].(type) {
		case nil:
			delete(m, key)
		case []any:
			if len(value) == 0 {
				delete(m, key)
			}
		case map[string]any:
			if len(value) == 0 {
				delete(m, key)
			}
		}
	}
	for _, key := range []string{"identities", "assertions"} {
		empty(root, key)
	}
	if env, ok := root["environment"].(map[string]any); ok {
		vars, _ := env["variables"].(map[string]any)
		if len(vars) == 0 {
			delete(root, "environment")
		}
	}
	profiles, _ := root["clientProfiles"].([]any)
	for _, raw := range profiles {
		if p, ok := raw.(map[string]any); ok {
			empty(p, "headers")
		}
	}
	groups, _ := root["groups"].([]any)
	for _, raw := range groups {
		g, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		delete(g, "executor") // The admitted executable contract supports once only.
		if identity, ok := g["identityAssignment"].(map[string]any); ok && identity["identityId"] == nil {
			delete(g, "identityAssignment")
		}
		if lifecycle, ok := g["lifecycle"].(map[string]any); ok {
			if timeout, ok := domain.Integer(lifecycle["cleanupTimeoutMs"]); ok && timeout == 2000 {
				delete(lifecycle, "cleanupTimeoutMs")
			}
			for _, phase := range []string{"initialize", "iteration", "cleanup"} {
				if binding, ok := lifecycle[phase].(map[string]any); ok {
					empty(binding, "inputs")
				} else if lifecycle[phase] == nil {
					delete(lifecycle, phase)
				}
			}
		}
	}
	flows, _ := root["workflows"].([]any)
	for _, raw := range flows {
		w, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		empty(w, "inputs")
		empty(w, "outputs")
		nodes, _ := w["nodes"].([]any)
		for _, raw := range nodes {
			n, ok := raw.(map[string]any)
			if !ok {
				continue
			}
			if n["name"] == nil {
				delete(n, "name")
			}
			c, ok := n["config"].(map[string]any)
			if !ok {
				continue
			}
			switch n["type"] {
			case "http":
				for _, key := range []string{"headers", "baseUrl", "timeoutMs", "body"} {
					empty(c, key)
				}
			case "authentication":
				if c["headerName"] == "Authorization" {
					delete(c, "headerName")
				}
				if c["headerPrefix"] == "Bearer " {
					delete(c, "headerPrefix")
				}
				empty(c, "baseUrl")
			case "call", "exclusiveSplit":
				empty(c, "inputs")
			case "setSession":
				empty(c, "setHeaders")
				empty(c, "removeHeaders")
				if headers, ok := c["setHeaders"].(map[string]any); ok {
					for _, raw := range headers {
						if source, ok := raw.(map[string]any); ok && source["prefix"] == "" {
							delete(source, "prefix")
						}
					}
				}
			case "extract":
				if c["type"] == "string" {
					delete(c, "type")
				}
				if outputs, ok := c["outputs"].(map[string]any); ok {
					for _, raw := range outputs {
						if out, ok := raw.(map[string]any); ok && out["type"] == "string" {
							delete(out, "type")
						}
					}
				}
			}
		}
	}
	return root, nil
}
