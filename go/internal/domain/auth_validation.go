package domain

import (
	"fmt"
	"strings"
)

func authName(c *checks, name, path, node string) {
	if strings.TrimSpace(name) == "" || len(name) > 128 {
		c.add("invalidAuthName", path, "Authentication identifiers must be nonempty and at most 128 UTF-8 bytes.", node)
	}
}
func invalidHeaderText(v string) bool {
	for _, b := range []byte(v) {
		if b < 32 && b != '\t' || b == 127 {
			return true
		}
	}
	return false
}
func (c *compiler) validateAuth(local *checks, w *Workflow, g *graph, path string) {
	ids := []string{}
	identities := map[string]Identity{}
	for _, identity := range c.doc.Identities {
		ids = append(ids, identity.ID)
		identities[identity.ID] = identity
	}
	local.ids(ids, "/identities")
	for i, id := range c.doc.Identities {
		p := fmt.Sprintf("/identities/%d", i)
		authName(local, id.ID, p+"/id", "")
		for _, field := range keys(id.Credentials) {
			p := Pointer(p+"/credentials", field)
			authName(local, field, p, "")
			if !Identifier(id.Credentials[field].Name) {
				local.add("invalidSecretReference", p+"/name", "Environment references require an ASCII variable name of at most 128 bytes.")
			}
		}
	}
	for i, group := range c.doc.Groups {
		if group.IdentityAssignment.IdentityID != nil {
			id := *group.IdentityAssignment.IdentityID
			p := fmt.Sprintf("/groups/%d/identityAssignment/identityId", i)
			authName(local, id, p, "")
			if _, ok := identities[id]; !ok {
				local.add("missingIdentity", p, "Assigned identity does not exist.")
			}
		}
	}
	fields := map[string]bool{}
	earlier := func(source, p, node string) (int, bool) {
		authName(local, source, p, node)
		index, ok := g.indices[source]
		if !ok {
			local.add("missingSourceNode", p, "Referenced source node does not exist.", node)
			return 0, false
		}
		if !g.dominates(source, node) {
			local.add("forwardReference", p, "Source must precede this node in workflow connectivity.", node)
			return 0, false
		}
		return index, true
	}
	extractionSource := func(s Source, p, node string) {
		authName(local, s.Name, p+"/name", node)
		if index, ok := earlier(s.NodeID, p+"/nodeId", node); ok {
			producer := w.Nodes[index]
			if producer.Type != "extract" {
				local.add("invalidSourceType", p+"/nodeId", "Output references must point to an earlier extraction node.", node)
			} else if _, ok := producer.Config.ExtractEntries()[s.Name]; !ok {
				local.add("missingOutput", p+"/name", "Referenced extraction does not declare this output.", node)
			}
		}
	}
	for _, index := range g.order {
		n := w.Nodes[index]
		v := n.Config
		p := fmt.Sprintf("%s/nodes/%d/config", path, index)
		switch n.Type {
		case "http":
			if v.Body != nil && v.Body.Kind == "jsonTemplate" {
				var walk func(Source, string)
				walk = func(s Source, p string) {
					switch s.Source {
					case "identity":
						authName(local, s.Field, p+"/field", n.ID)
						fields[s.Field] = true
					case "extraction":
						extractionSource(s, p, n.ID)
					case "object":
						for _, name := range keys(s.Fields) {
							walk(s.Fields[name], Pointer(p+"/fields", name))
						}
					case "array":
						for i, s := range s.Items {
							walk(s, fmt.Sprintf("%s/items/%d", p, i))
						}
					}
				}
				for _, name := range keys(v.Body.Fields) {
					walk(v.Body.Fields[name], Pointer(p+"/body/fields", name))
				}
			}
		case "extract":
			authName(local, n.ID, fmt.Sprintf("%s/nodes/%d/id", path, index), n.ID)
			if len(v.Outputs) > 32 {
				local.add("extractionOutputLimit", p+"/outputs", "An Extract node supports at most 32 outputs.", n.ID)
			}
			if len(v.Outputs) > 0 && (v.Expression != "" || v.Output != "") {
				local.add("mixedExtractionConfig", p, "Use either named outputs or a single expression/output pair.", n.ID)
			}
			for _, name := range keys(v.ExtractEntries()) {
				out := v.ExtractEntries()[name]
				op := p + "/output"
				ep := p + "/expression"
				if len(v.Outputs) > 0 {
					op = Pointer(p+"/outputs", name)
					ep = op + "/expression"
				}
				authName(local, name, op, n.ID)
				if _, e := c.expressions.CompileExtraction(out.Expression); e != nil {
					category := "validation"
					if e.Code == "expressionLimit" {
						category = "resourceLimit"
					}
					local.expression(e, ep, out.Expression, category, n.ID)
				}
			}
			if source, ok := earlier(v.SourceNodeID, p+"/sourceNodeId", n.ID); ok && w.Nodes[source].Type != "http" {
				local.add("invalidSourceType", p+"/sourceNodeId", "Extraction sources must be earlier HTTP nodes.", n.ID)
			}
		case "setSession":
			authName(local, n.ID, fmt.Sprintf("%s/nodes/%d/id", path, index), n.ID)
			if len(v.SetHeaders) == 0 && len(v.RemoveHeaders) == 0 {
				local.add("emptySessionUpdate", p, "Declare at least one header set or removal operation.", n.ID)
			}
			set := map[string]bool{}
			removed := map[string]bool{}
			checkName := func(name, p string) {
				if len(name) > 128 || !HeaderName(name) {
					local.add("invalidHeader", p, "Use an HTTP token header name of at most 128 bytes.", n.ID)
				}
			}
			for _, name := range keys(v.SetHeaders) {
				s := v.SetHeaders[name]
				hp := Pointer(p+"/setHeaders", name)
				checkName(name, hp)
				normalized := strings.ToLower(name)
				if set[normalized] {
					local.add("duplicateHeaderOperation", hp, "Header names must be unique ignoring case.", n.ID)
				}
				set[normalized] = true
				switch s.Source {
				case "literal":
					value, _ := s.Value.(string)
					if len(value)+len(s.Prefix) > 8192 || invalidHeaderText(value) {
						local.add("invalidHeaderValue", hp+"/value", "Rendered literal header values exceed limits or contain prohibited control characters.", n.ID)
					}
				case "identity":
					authName(local, s.Field, hp+"/field", n.ID)
					fields[s.Field] = true
				case "extraction":
					extractionSource(s, hp, n.ID)
				}
				if len(s.Prefix) > 8192 || invalidHeaderText(s.Prefix) {
					local.add("invalidHeaderValue", hp+"/prefix", "Header prefixes exceed limits or contain prohibited control characters.", n.ID)
				}
			}
			for i, name := range v.RemoveHeaders {
				hp := fmt.Sprintf("%s/removeHeaders/%d", p, i)
				checkName(name, hp)
				normalized := strings.ToLower(name)
				if removed[normalized] {
					local.add("duplicateHeaderOperation", hp, "Header removals must be unique ignoring case.", n.ID)
				}
				removed[normalized] = true
				if set[normalized] {
					local.add("conflictingHeaderOperation", hp, "A header cannot be set and removed in the same update.", n.ID)
				}
			}
		}
	}
	for i, group := range c.doc.Groups {
		p := fmt.Sprintf("/groups/%d/identityAssignment/identityId", i)
		if group.IdentityAssignment.IdentityID == nil {
			if len(fields) > 0 {
				local.add("missingAssignedIdentity", p, "This workflow consumes identity fields; each group must have an assigned identity.")
			}
			continue
		}
		if identity, ok := identities[*group.IdentityAssignment.IdentityID]; ok {
			for _, field := range keys(fields) {
				if _, ok := identity.Credentials[field]; !ok {
					local.add("missingCredentialField", p, "Assigned identity does not declare every credential field consumed by the workflow.")
				}
			}
		}
	}
}
