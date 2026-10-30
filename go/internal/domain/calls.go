package domain

import (
	"strconv"
	"strings"
)

type callSummary struct{ depth, calls, nodes int }

// reachable validates even unused call trees. Memoized summaries avoid expanding
// repeated subworkflows exponentially before invocation budgets can reject them.
func (c *compiler) reachable(l Lifecycle) []int {
	indices := map[string]int{}
	for i, w := range c.doc.Workflows {
		indices[w.ID] = i
	}
	active := map[int]bool{}
	memo := map[int]callSummary{}
	var visit func(int) (callSummary, bool)
	visit = func(i int) (callSummary, bool) {
		if active[i] {
			c.add("recursiveWorkflow", "/workflows", "Recursive workflow calls are unsupported.")
			return callSummary{}, false
		}
		if v, ok := memo[i]; ok {
			return v, true
		}
		active[i] = true
		v := callSummary{1, 1, len(c.flows[i].Order)}
		for _, n := range c.doc.Workflows[i].Nodes {
			if n.Type == "call" {
				child, ok := indices[n.Config.WorkflowID]
				if !ok {
					c.add("missingWorkflow", "/workflows", "Called workflow does not exist.")
					return v, false
				}
				childSummary, ok := visit(child)
				if !ok {
					return v, false
				}
				v.depth = max(v.depth, childSummary.depth+1)
				v.calls += childSummary.calls
				v.nodes += childSummary.nodes
			}
		}
		delete(active, i)
		if v.depth > 8 || v.calls > 128 || v.nodes > 1024 {
			c.add("invocationLimit", "/workflows", "Workflow invocation depth, calls, or visits exceed limits.")
			return v, false
		}
		memo[i] = v
		return v, true
	}
	for i := range c.doc.Workflows {
		if _, ok := visit(i); !ok {
			return nil
		}
	}
	roots := []int{}
	calls, nodes := 0, 0
	for _, b := range []*Binding{l.Initialize, &l.Iteration, l.Cleanup} {
		if b != nil {
			i := indices[b.WorkflowID]
			roots = append(roots, i)
			calls += memo[i].calls
			nodes += memo[i].nodes
		}
	}
	if calls > 128 || nodes > 1024 {
		c.add("invocationLimit", "/groups", "Lifecycle exceeds aggregate call/visit limits.")
		return nil
	}
	seen := map[int]bool{}
	pending := roots
	for len(pending) > 0 {
		i := pending[len(pending)-1]
		pending = pending[:len(pending)-1]
		if seen[i] {
			continue
		}
		seen[i] = true
		for _, n := range c.doc.Workflows[i].Nodes {
			if n.Type == "call" {
				pending = append(pending, indices[n.Config.WorkflowID])
			}
		}
	}
	result := []int{}
	for i := range c.doc.Workflows {
		if seen[i] {
			result = append(result, i)
		}
	}
	return result
}
func (c *compiler) secrets(reachable []int) []RequiredSecret {
	var identity *Identity
	assignment := c.doc.Groups[0].IdentityAssignment.IdentityID
	if assignment != nil {
		for i := range c.doc.Identities {
			if c.doc.Identities[i].ID == *assignment {
				identity = &c.doc.Identities[i]
			}
		}
	}
	for _, field := range keys(c.identityFields) {
		if identity == nil {
			c.add("missingIdentityCredential", "/identities", "Assigned identity does not declare a consumed field.")
			return nil
		}
		if _, ok := identity.Credentials[field]; !ok {
			c.add("missingIdentityCredential", "/identities", "Assigned identity does not declare a consumed field.")
			return nil
		}
	}
	fields := map[string]bool{}
	record := func(s Source) {
		if s.Source == "identity" {
			fields[s.Field] = true
		}
	}
	for _, i := range reachable {
		w := c.doc.Workflows[i]
		for _, n := range w.Nodes {
			visitSources(n, record)
		}
		for _, name := range keys(w.Outputs) {
			record(w.Outputs[name].Value)
		}
	}
	l := c.doc.Groups[0].Lifecycle
	for _, b := range []*Binding{l.Initialize, &l.Iteration, l.Cleanup} {
		if b != nil {
			for _, name := range keys(b.Inputs) {
				record(b.Inputs[name])
			}
		}
	}
	result := []RequiredSecret{}
	for _, field := range keys(fields) {
		if identity == nil {
			c.add("missingIdentity", "/groups", "Assigned identity is required.")
			return nil
		}
		result = append(result, RequiredSecret{IdentityID: identity.ID, Field: field, Reference: identity.Credentials[field]})
	}
	for _, wi := range reachable {
		for ni, n := range c.doc.Workflows[wi].Nodes {
			if n.Type == "authentication" {
				id := strings.Repeat("authentication", 11) + ":" + strconv.Itoa(wi) + ":" + strconv.Itoa(ni)
				result = append(result, RequiredSecret{IdentityID: id, Field: "username", Reference: n.Config.Username}, RequiredSecret{IdentityID: id, Field: "password", Reference: n.Config.Password})
			}
		}
	}
	return result
}
