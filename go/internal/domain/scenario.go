package domain

import "encoding/json"

// Document is the canonical, transport-independent scenario. Codecs admit strict
// wire shapes before constructing it; validation never reads process credentials.
type Document struct {
	// SourceJSON retains the admitted wire shape for safe report snapshots only.
	// It is not persisted as a scenario field or interpreted during execution.
	SourceJSON     []byte          `json:"-"`
	SchemaVersion  uint32          `json:"schemaVersion"`
	ID             string          `json:"id"`
	Name           string          `json:"name"`
	Environment    Environment     `json:"environment"`
	ClientProfiles []ClientProfile `json:"clientProfiles"`
	Identities     []Identity      `json:"identities"`
	Groups         []Group         `json:"groups"`
	Workflows      []Workflow      `json:"workflows"`
}
type Environment struct {
	Variables map[string]any `json:"variables"`
}
type ClientProfile struct {
	ID        string            `json:"id"`
	BaseURL   string            `json:"baseUrl"`
	TimeoutMS uint64            `json:"timeoutMs"`
	Headers   map[string]string `json:"headers"`
}
type SecretReference struct {
	Source string `json:"source"`
	Name   string `json:"name"`
}
type Identity struct {
	ID          string                     `json:"id"`
	Credentials map[string]SecretReference `json:"credentials"`
}
type Group struct {
	ID                 string `json:"id"`
	ClientProfileID    string `json:"clientProfileId"`
	IdentityAssignment struct {
		Mode       string  `json:"mode"`
		IdentityID *string `json:"identityId"`
	} `json:"identityAssignment"`
	Lifecycle Lifecycle `json:"lifecycle"`
}
type Lifecycle struct {
	Initialize       *Binding `json:"initialize"`
	Iteration        Binding  `json:"iteration"`
	Cleanup          *Binding `json:"cleanup"`
	CleanupTimeoutMS uint64   `json:"cleanupTimeoutMs"`
}
type Binding struct {
	WorkflowID string            `json:"workflowId"`
	Inputs     map[string]Source `json:"inputs"`
}
type InputPort struct {
	Type    string          `json:"type"`
	Default json.RawMessage `json:"default,omitempty"`
}
type OutputPort struct {
	Type  string `json:"type"`
	Value Source `json:"value"`
}
type Workflow struct {
	ID      string                `json:"id"`
	Name    string                `json:"name"`
	Inputs  map[string]InputPort  `json:"inputs"`
	Outputs map[string]OutputPort `json:"outputs"`
	Nodes   []Node                `json:"nodes"`
	Edges   []Edge                `json:"edges"`
	Layout  *Layout               `json:"layout"`
}

// Node carries an admitted operation. Its configuration's active fields are
// selected by Type; codecs reject fields belonging to another operation variant.
type Node struct {
	ID     string  `json:"id"`
	Name   *string `json:"name"`
	Type   string  `json:"type"`
	Config Config  `json:"config"`
}
type Config struct {
	BaseURL         *string                  `json:"baseUrl"`
	Method          string                   `json:"method"`
	Path            string                   `json:"path"`
	Headers         map[string]string        `json:"headers"`
	TimeoutMS       *uint64                  `json:"timeoutMs"`
	Body            *Body                    `json:"body"`
	Mode            string                   `json:"mode"`
	Username        SecretReference          `json:"username"`
	Password        SecretReference          `json:"password"`
	UsernameKey     string                   `json:"usernameKey"`
	PasswordKey     string                   `json:"passwordKey"`
	TokenExpression string                   `json:"tokenExpression"`
	HeaderName      string                   `json:"headerName"`
	HeaderPrefix    string                   `json:"headerPrefix"`
	WorkflowID      string                   `json:"workflowId"`
	Inputs          map[string]Source        `json:"inputs"`
	SourceNodeID    string                   `json:"sourceNodeId"`
	Expression      string                   `json:"expression"`
	Output          string                   `json:"output"`
	OutputType      string                   `json:"type"`
	Outputs         map[string]ExtractOutput `json:"outputs"`
	SetHeaders      map[string]Source        `json:"setHeaders"`
	RemoveHeaders   []string                 `json:"removeHeaders"`
}
type ExtractOutput struct {
	Expression string `json:"expression"`
	Type       string `json:"type"`
}
type Body struct {
	Kind   string            `json:"kind"`
	Value  any               `json:"value"`
	Fields map[string]Source `json:"fields"`
}

// Source represents explicit scalar/template descriptors. Literal values are
// opaque JSON, even when their objects resemble descriptors. Only object/array
// source variants recursively compose templates.
type Source struct {
	Source string            `json:"source"`
	Value  any               `json:"value"`
	Name   string            `json:"name"`
	Field  string            `json:"field"`
	NodeID string            `json:"nodeId"`
	Phase  string            `json:"phase"`
	Prefix string            `json:"prefix"`
	Fields map[string]Source `json:"fields"`
	Items  []Source          `json:"items"`
}
type Edge struct {
	ID        string     `json:"id"`
	From      string     `json:"from"`
	To        string     `json:"to"`
	Condition *Condition `json:"condition"`
	Default   *bool      `json:"default"`
}
type Condition struct {
	Expression string `json:"expression"`
}
type Layout struct {
	Positions   map[string]Position   `json:"positions"`
	Connections map[string]Connection `json:"connections"`
}
type Position struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}
type Connection struct {
	Source string `json:"source"`
	Target string `json:"target"`
}

// ExtractEntries normalizes the legacy single-output and named-output shapes.
func (c Config) ExtractEntries() map[string]ExtractOutput {
	if len(c.Outputs) > 0 {
		return c.Outputs
	}
	kind := c.OutputType
	if kind == "" {
		kind = "string"
	}
	return map[string]ExtractOutput{c.Output: {Expression: c.Expression, Type: kind}}
}
