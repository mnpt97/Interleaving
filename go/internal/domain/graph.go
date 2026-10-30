package domain

import (
	"fmt"
	"strings"
)

// FlowPlan contains only checked graph/dataflow metadata. Edges define order;
// canvas coordinates never influence compilation. Pairing is inferred, not stored.
type FlowPlan struct {
	Order            []int
	Pairs            map[int]int
	Dominators       map[string]map[string]bool
	Predicates       map[int]Predicate
	Extractions      map[string]map[string][]string
	ExtractionValues map[string]map[string]ScalarExpression
	AuthTokens       map[string]ScalarExpression
	LastUses         map[string]string
	SensitiveHTTP    map[string]bool
	ResponseLastUses map[string]string
}
type graph struct {
	indices map[string]int
	out, in [][]int
	order   []int
	pairs   map[int]int
	dom     map[string]map[string]bool
}
type checks struct {
	errors   Diagnostics
	workflow *string
}

func (c *checks) add(code, path, message string, node ...string) {
	c.problem("validation", code, path, message, node...)
}
func (c *checks) problem(category, code, path, message string, node ...string) {
	if len(c.errors) >= 100 {
		return
	}
	d := Problem(category, code, path, message)
	d.WorkflowID = c.workflow
	if len(node) > 0 && node[0] != "" {
		v := node[0]
		d.NodeID = &v
	}
	c.errors = append(c.errors, d)
}
func (c *checks) id(id, path string) {
	if strings.TrimSpace(id) == "" || len(id) > 128 {
		c.add("invalidId", path, "IDs must be nonempty and at most 128 UTF-8 bytes.")
	}
}
func Identifier(id string) bool {
	if id == "" || len(id) > 128 {
		return false
	}
	for i, b := range []byte(id) {
		if !(b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b == '_' || i > 0 && b >= '0' && b <= '9') {
			return false
		}
	}
	return true
}
func (c *checks) identifier(id, path string) {
	if !Identifier(id) {
		c.add("invalidPortName", path, "Use an ASCII identifier of at most 128 bytes.")
	}
}
func (c *checks) ids(ids []string, path string) bool {
	seen := map[string]bool{}
	valid := true
	for i, id := range ids {
		p := fmt.Sprintf("%s/%d/id", path, i)
		if strings.TrimSpace(id) == "" {
			c.add("emptyId", p, "IDs must not be empty.")
			valid = false
		}
		if seen[id] {
			c.add("duplicateId", p, "IDs must be unique within their collection.")
			valid = false
		}
		seen[id] = true
	}
	return valid
}
func HeaderName(name string) bool {
	if name == "" {
		return false
	}
	for _, b := range []byte(name) {
		if !(b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b >= '0' && b <= '9' || strings.ContainsRune("!#$%&'*+-.^_`|~", rune(b))) {
			return false
		}
	}
	return true
}
func (c *checks) headers(headers map[string]string, path, node string) {
	for _, name := range keys(headers) {
		if !HeaderName(name) || strings.ContainsAny(headers[name], "\r\n") {
			c.add("invalidHeader", Pointer(path, name), "Use a valid HTTP header name and a value without line breaks.", node)
		}
	}
}

// validateGraph establishes safe traversal invariants before any reference pass.
// Kahn's algorithm detects cycles across all components, not only reachable nodes.
func (c *checks) validateGraph(w *Workflow, path string) *graph {
	g := &graph{indices: map[string]int{}, out: make([][]int, len(w.Nodes)), in: make([][]int, len(w.Nodes)), pairs: map[int]int{}}
	starts := []int{}
	ends := 0
	ids := []string{}
	for i, n := range w.Nodes {
		g.indices[n.ID] = i
		ids = append(ids, n.ID)
		if n.Type == "start" {
			starts = append(starts, i)
		}
		if n.Type == "end" {
			ends++
		}
	}
	if !c.ids(ids, path+"/nodes") {
		return nil
	}
	if len(starts) != 1 {
		c.add("startCount", path+"/nodes", "Workflow requires exactly one start node.")
	}
	if ends != 1 {
		c.add("endCount", path+"/nodes", "Workflow requires exactly one end node.")
	}
	edges := map[[2]int]bool{}
	dangling := false
	for i, e := range w.Edges {
		p := fmt.Sprintf("%s/edges/%d", path, i)
		a, aok := g.indices[e.From]
		b, bok := g.indices[e.To]
		if !aok || !bok {
			c.add("danglingEdge", p, "Both edge endpoints must reference existing nodes.")
			dangling = true
			continue
		}
		pair := [2]int{a, b}
		if edges[pair] && w.Nodes[a].Type != "exclusiveSplit" {
			c.add("duplicateEdge", p, "Duplicate workflow edge.", e.From)
			continue
		}
		edges[pair] = true
		if a == b {
			c.add("selfLoop", p, "Self-loops are unsupported.", e.From)
		}
		g.out[a] = append(g.out[a], b)
		g.in[b] = append(g.in[b], a)
	}
	if dangling {
		return nil
	}
	for i, n := range w.Nodes {
		a, b := len(g.in[i]), len(g.out[i])
		valid := a == 1 && b == 1
		switch n.Type {
		case "start":
			valid = a == 0 && b == 1
		case "end":
			valid = a == 1 && b == 0
		case "exclusiveSplit":
			valid = a == 1 && b >= 2 && b <= 8
		case "exclusiveMerge":
			valid = a >= 2 && a <= 8 && b == 1
		}
		if !valid {
			c.add("nonLinearGraph", fmt.Sprintf("%s/nodes/%d", path, i), "Node connections do not meet its input/output requirements.", n.ID)
		}
	}
	degree := make([]int, len(w.Nodes))
	queue := []int{}
	for i := range degree {
		degree[i] = len(g.in[i])
		if degree[i] == 0 {
			queue = append(queue, i)
		}
	}
	for len(queue) > 0 {
		i := queue[0]
		queue = queue[1:]
		g.order = append(g.order, i)
		for _, next := range g.out[i] {
			degree[next]--
			if degree[next] == 0 {
				queue = append(queue, next)
			}
		}
	}
	if len(g.order) != len(w.Nodes) {
		c.add("cycle", path+"/edges", "Cycles are unsupported in the current schema.")
	}
	if len(g.order) == len(w.Nodes) && len(c.errors) == 0 {
		g.computeDominators(w)
		c.regions(w, g, path)
	}
	if len(starts) == 1 {
		seen := g.reachable(starts[0], -1)
		for i, n := range w.Nodes {
			if !seen[i] {
				c.add("unreachableNode", fmt.Sprintf("%s/nodes/%d", path, i), "Node is unreachable from the start.", n.ID)
			}
		}
	}
	if len(c.errors) > 0 {
		return nil
	}
	return g
}
func (g *graph) reachable(start, stop int) map[int]bool {
	seen := map[int]bool{}
	pending := []int{start}
	for len(pending) > 0 {
		i := pending[len(pending)-1]
		pending = pending[:len(pending)-1]
		if seen[i] || i == stop {
			continue
		}
		seen[i] = true
		pending = append(pending, g.out[i]...)
	}
	return seen
}
func cloneSet[K comparable](s map[K]bool) map[K]bool {
	r := map[K]bool{}
	for k := range s {
		r[k] = true
	}
	return r
}
func intersect[K comparable](a, b map[K]bool) {
	for k := range a {
		if !b[k] {
			delete(a, k)
		}
	}
}
func (g *graph) computeDominators(w *Workflow) {
	g.dom = map[string]map[string]bool{}
	for _, i := range g.order {
		common := map[string]bool{}
		for j, p := range g.in[i] {
			if j == 0 {
				common = cloneSet(g.dom[w.Nodes[p].ID])
			} else {
				intersect(common, g.dom[w.Nodes[p].ID])
			}
		}
		common[w.Nodes[i].ID] = true
		g.dom[w.Nodes[i].ID] = common
	}
}
func (g *graph) dominates(producer, consumer string) bool {
	return producer != consumer && g.dom[consumer][producer]
}
func (c *checks) regions(w *Workflow, g *graph, path string) {
	post := make([]map[int]bool, len(w.Nodes))
	for k := len(g.order) - 1; k >= 0; k-- {
		i := g.order[k]
		common := map[int]bool{}
		for j, n := range g.out[i] {
			if j == 0 {
				common = cloneSet(post[n])
			} else {
				intersect(common, post[n])
			}
		}
		common[i] = true
		post[i] = common
	}
	for i, n := range w.Nodes {
		if n.Type != "exclusiveSplit" {
			continue
		}
		for _, candidate := range g.order {
			if candidate != i && post[i][candidate] {
				if w.Nodes[candidate].Type == "exclusiveMerge" {
					g.pairs[i] = candidate
				}
				break
			}
		}
	}
	for i, n := range w.Nodes {
		p := fmt.Sprintf("%s/nodes/%d", path, i)
		if n.Type != "exclusiveSplit" {
			if n.Type == "exclusiveMerge" {
				count := 0
				for _, m := range g.pairs {
					if m == i {
						count++
					}
				}
				if count != 1 {
					c.add("invalidGatewayPair", p, "Connect this merge to branches from exactly one split.", n.ID)
				}
			}
			continue
		}
		merge, ok := g.pairs[i]
		if !ok {
			c.add("invalidGatewayPair", p, "All split branches must reconverge at one exclusive merge before continuing.", n.ID)
			continue
		}
		source, sok := g.indices[n.Config.SourceNodeID]
		if !sok || w.Nodes[source].Type != "http" || !g.dominates(n.Config.SourceNodeID, n.ID) {
			c.add("routeSourceUnavailable", p+"/config/sourceNodeId", "Source must be an HTTP node available on every path to the split.", n.ID)
		}
		if !g.dominates(n.ID, w.Nodes[merge].ID) {
			c.add("unstructuredBranch", path+"/edges", "The split must dominate its paired merge.", n.ID)
		}
		occupied := map[int]bool{}
		for _, arm := range g.out[i] {
			seen := g.reachable(arm, merge)
			reached := arm == merge
			overlap := false
			for j := range seen {
				if occupied[j] {
					overlap = true
				}
				if len(g.out[j]) == 0 {
					c.add("unstructuredBranch", path+"/edges", "Every arm must reach its paired merge before End.", n.ID)
				}
				for _, next := range g.out[j] {
					if next == merge {
						reached = true
					}
				}
			}
			if !reached || overlap {
				c.add("unstructuredBranch", path+"/edges", "Branch arms must be disjoint and converge only at their paired merge.", n.ID)
			}
			for j := range seen {
				if w.Nodes[j].Type == "exclusiveSplit" {
					m, ok := g.pairs[j]
					if !ok || !seen[m] {
						c.add("unstructuredBranch", path+"/edges", "Nested split/merge regions must be properly contained.", n.ID)
					}
				}
				occupied[j] = true
			}
		}
	}
	for i, e := range w.Edges {
		split := w.Nodes[g.indices[e.From]].Type == "exclusiveSplit"
		valid := e.Condition == nil && e.Default == nil
		if split {
			valid = e.Condition != nil && e.Default == nil || e.Condition == nil && e.Default != nil && *e.Default
		}
		if !valid {
			c.add("invalidRouteEdge", fmt.Sprintf("%s/edges/%d", path, i), "Split edges require a condition or default:true; ordinary edges allow neither.")
		}
	}
	for _, n := range w.Nodes {
		if n.Type == "exclusiveSplit" {
			count := 0
			for _, e := range w.Edges {
				if e.From == n.ID && e.Default != nil && *e.Default {
					count++
				}
			}
			if count > 1 {
				c.add("multipleDefaultRoutes", path+"/edges", "A split permits at most one default route.", n.ID)
			}
		}
	}
}
