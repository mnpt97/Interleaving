// Package scenariocodec implements duplicate-aware, bounded scenario admission.
// The embedded schema is a checked copy of the existing shared wire contract.
package scenariocodec

import (
	"bytes"
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"io"
	"math"
	"strconv"
	"strings"
	"unicode/utf8"

	schema "github.com/santhosh-tekuri/jsonschema/v6"
	"github.com/santhosh-tekuri/jsonschema/v6/kind"
	"go.yaml.in/yaml/v3"
	"interleaving/internal/domain"
)

//go:embed scenario.schema.json
var schemaSource []byte

// Limits applies independently to each document. Byte limits also bound the YAML
// library's syntax tree; aliases are never expanded or unmarshaled into objects.
type Limits struct{ Bytes, Depth, Collection, Values int }

var DefaultLimits = Limits{1024 * 1024, 64, 10000, 100000}

type Codec struct {
	shape  *schema.Schema
	limits Limits
}

func New() (*Codec, error) { return NewWithLimits(DefaultLimits) }
func NewWithLimits(limits Limits) (*Codec, error) {
	var value any
	if err := json.Unmarshal(schemaSource, &value); err != nil {
		return nil, err
	}
	compiler := schema.NewCompiler()
	// All references are local; no external schema loader is registered.
	if err := compiler.AddResource("urn:interleaving:scenario", value); err != nil {
		return nil, err
	}
	shape, err := compiler.Compile("urn:interleaving:scenario")
	return &Codec{shape: shape, limits: limits}, err
}
func fail(category, code, path string) error {
	message := "Field has an invalid type or unsupported configuration; see the scenario schema contract."
	switch code {
	case "fileSizeLimit":
		message = "Scenario source exceeds the configured byte limit."
	case "resourceLimit":
		message = "Scenario nesting or aggregate value limit exceeded."
	case "collectionLimit":
		message = "Scenario collection size limit exceeded."
	case "invalidUtf8":
		message = "Scenario files must use UTF-8."
	case "invalidJson":
		message = "Invalid JSON syntax or number."
	case "invalidYaml":
		message = "Invalid YAML syntax."
	case "duplicateKey":
		message = "Duplicate object key."
	case "trailingContent":
		message = "Unexpected content after the JSON document."
	case "missingVersion":
		message = "Declare schemaVersion: 2."
	case "unsupportedVersion":
		message = "Only integer schemaVersion: 2 is supported."
	case "unsupportedFeature":
		message = "Nonempty assertions are unsupported; use an empty array."
	case "unsupportedExtension":
		message = "Use a JSON or YAML scenario file."
	case "emptyDocument":
		message = "Provide one scenario document."
	case "multipleDocuments":
		message = "Only one YAML document is allowed."
	case "yamlAlias":
		message = "YAML aliases are unsupported."
	case "yamlMetadata":
		message = "YAML anchors and explicit tags are unsupported."
	case "yamlMerge":
		message = "YAML merge keys are unsupported."
	case "nonStringKey":
		message = "YAML object keys must be strings."
	case "ambiguousScalar":
		message = "Quote ambiguous YAML scalars or use a JSON-compatible primitive."
	}
	d := domain.Problem(category, code, path, message)
	return d
}
func (c *Codec) Decode(ctx context.Context, source []byte, format string) (*domain.Document, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if len(source) > c.limits.Bytes {
		return nil, fail("resourceLimit", "fileSizeLimit", "")
	}
	if !utf8.Valid(source) {
		return nil, fail("parse", "invalidUtf8", "")
	}
	b := budget{limits: c.limits, ctx: ctx, source: source}
	var value any
	var err error
	switch format {
	case "json":
		dec := json.NewDecoder(bytes.NewReader(source))
		dec.UseNumber()
		value, err = b.jsonValue(dec, "", 0)
		if err == nil {
			if _, end := dec.Token(); end != io.EOF {
				err = fail("parse", "trailingContent", "")
			}
		}
		if err != nil {
			offset := int(dec.InputOffset())
			var syntax *json.SyntaxError
			if errors.As(err, &syntax) {
				offset = int(syntax.Offset) - 1
			}
			var d domain.Diagnostic
			if !errors.As(err, &d) && ctx.Err() == nil {
				err = fail("parse", "invalidJson", "")
			}
			if errors.As(err, &d) {
				line, column := 1, 1
				for i, b := range source {
					if i >= offset {
						break
					}
					if b == '\n' {
						line++
						column = 1
					} else {
						column++
					}
				}
				d.Line = &line
				d.Column = &column
				err = d
			}
		}
	case "yaml", "yml":
		dec := yaml.NewDecoder(bytes.NewReader(source))
		var node yaml.Node
		if e := dec.Decode(&node); e != nil {
			if e == io.EOF {
				err = fail("parse", "emptyDocument", "")
			} else {
				err = fail("parse", "invalidYaml", "")
			}
		} else if len(node.Content) == 0 {
			err = fail("parse", "emptyDocument", "")
		} else {
			value, err = b.yamlValue(node.Content[0], "", 0, false)
		}
		if err == nil {
			var next yaml.Node
			if e := dec.Decode(&next); e != io.EOF {
				if e == nil {
					err = fail("parse", "multipleDocuments", "")
				} else {
					err = fail("parse", "invalidYaml", "")
				}
			}
		}
	default:
		return nil, fail("format", "unsupportedExtension", "")
	}
	if err != nil {
		return nil, err
	}
	root, _ := value.(map[string]any)
	v, exists := root["schemaVersion"]
	if !exists {
		return nil, fail("schema", "missingVersion", "/schemaVersion")
	}
	if v != json.Number("2") {
		return nil, fail("schema", "unsupportedVersion", "/schemaVersion")
	}
	if a, ok := root["assertions"].([]any); ok && len(a) > 0 {
		return nil, fail("schema", "unsupportedFeature", "/assertions")
	}
	if err = c.shape.Validate(value); err != nil {
		path := ""
		var v *schema.ValidationError
		if errors.As(err, &v) {
			for len(v.Causes) == 1 {
				v = v.Causes[0]
			}
			for _, segment := range v.InstanceLocation {
				path = domain.Pointer(path, segment)
			}
			if extra, ok := v.ErrorKind.(*kind.AdditionalProperties); ok && len(extra.Properties) > 0 {
				path = domain.Pointer(path, extra.Properties[0])
			}
			if len(v.InstanceLocation) == 4 && v.InstanceLocation[0] == "workflows" && v.InstanceLocation[2] == "nodes" {
				var at any = value
				for _, segment := range v.InstanceLocation {
					switch x := at.(type) {
					case map[string]any:
						at = x[segment]
					case []any:
						i, _ := strconv.Atoi(segment)
						if i < len(x) {
							at = x[i]
						}
					}
				}
				if n, ok := at.(map[string]any); ok {
					if typ, ok := n["type"].(string); ok && !strings.Contains("|start|end|http|authentication|extract|setSession|call|exclusiveSplit|exclusiveMerge|", "|"+typ+"|") {
						path = domain.Pointer(path, "type")
					}
				}
			}
		}
		return nil, fail("schema", "invalidSchema", path)
	}
	if err := checkUnsignedFields(root); err != nil {
		return nil, err
	}
	data, _ := json.Marshal(value)
	var document domain.Document
	document.SourceJSON = append([]byte(nil), data...)
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	if err := dec.Decode(&document); err != nil {
		path := ""
		var mismatch *json.UnmarshalTypeError
		if errors.As(err, &mismatch) {
			path = "/" + strings.ReplaceAll(mismatch.Field, ".", "/")
		}
		return nil, fail("schema", "invalidSchema", path)
	}
	// Defaults are applied only on absence, preserving explicit zero/null rejection.
	groups := root["groups"].([]any)
	for i := range document.Groups {
		g := groups[i].(map[string]any)
		l := g["lifecycle"].(map[string]any)
		if _, ok := l["cleanupTimeoutMs"]; !ok {
			document.Groups[i].Lifecycle.CleanupTimeoutMS = 2000
		}
	}
	workflows := root["workflows"].([]any)
	for wi := range document.Workflows {
		nodes := workflows[wi].(map[string]any)["nodes"].([]any)
		for ni := range document.Workflows[wi].Nodes {
			n := &document.Workflows[wi].Nodes[ni]
			config, _ := nodes[ni].(map[string]any)["config"].(map[string]any)
			if n.Type == "authentication" && n.Config.Mode == "bearer" {
				if _, ok := config["headerName"]; !ok {
					n.Config.HeaderName = "Authorization"
				}
				if _, ok := config["headerPrefix"]; !ok {
					n.Config.HeaderPrefix = "Bearer "
				}
			}
			if n.Type == "extract" {
				if n.Config.OutputType == "" {
					n.Config.OutputType = "string"
				}
				for name, out := range n.Config.Outputs {
					if out.Type == "" {
						out.Type = "string"
						n.Config.Outputs[name] = out
					}
				}
			}
		}
	}
	return &document, nil
}

// JSON Schema considers 1.0 and 1e0 integers, whereas the unsigned wire fields require
// integer syntax. Check their original number spellings before typed decoding;
// this also preserves array indices in diagnostics and exact uint64 boundaries.
func checkUnsignedFields(root map[string]any) error {
	check := func(value any, path string) error {
		n, ok := value.(json.Number)
		if !ok {
			return fail("schema", "invalidSchema", path)
		}
		if _, err := strconv.ParseUint(string(n), 10, 64); err != nil {
			return fail("schema", "invalidSchema", path)
		}
		return nil
	}
	for i, p := range root["clientProfiles"].([]any) {
		if err := check(p.(map[string]any)["timeoutMs"], "/clientProfiles/"+strconv.Itoa(i)+"/timeoutMs"); err != nil {
			return err
		}
	}
	for i, g := range root["groups"].([]any) {
		l := g.(map[string]any)["lifecycle"].(map[string]any)
		if v, ok := l["cleanupTimeoutMs"]; ok {
			if err := check(v, "/groups/"+strconv.Itoa(i)+"/lifecycle/cleanupTimeoutMs"); err != nil {
				return err
			}
		}
	}
	for wi, w := range root["workflows"].([]any) {
		for ni, n := range w.(map[string]any)["nodes"].([]any) {
			node := n.(map[string]any)
			if node["type"] != "http" {
				continue
			}
			config := node["config"].(map[string]any)
			if v, ok := config["timeoutMs"]; ok && v != nil {
				if err := check(v, "/workflows/"+strconv.Itoa(wi)+"/nodes/"+strconv.Itoa(ni)); err != nil {
					return err
				}
			}
		}
	}
	return nil
}

type budget struct {
	limits Limits
	values int
	ctx    context.Context
	source []byte
}

func (b *budget) count(path string, depth int) error {
	if err := b.ctx.Err(); err != nil {
		return err
	}
	b.values++
	if depth > b.limits.Depth || depth > 128 || b.values > b.limits.Values {
		return fail("resourceLimit", "resourceLimit", path)
	}
	return nil
}
func (b *budget) collection(path string, n int) error {
	if n > b.limits.Collection {
		return fail("resourceLimit", "collectionLimit", path)
	}
	return nil
}

// jsonValue charges before allocation and detects duplicate keys before insertion.
func (b *budget) jsonValue(d *json.Decoder, path string, depth int) (any, error) {
	if err := b.count(path, depth); err != nil {
		return nil, err
	}
	start := int(d.InputOffset())
	token, err := d.Token()
	if err != nil {
		return nil, err
	}
	if _, ok := token.(string); ok && !validJSONUnicode(b.source[start:int(d.InputOffset())]) {
		return nil, fail("parse", "invalidJson", "")
	}
	switch token {
	case json.Delim('{'):
		result := map[string]any{}
		for d.More() {
			start := int(d.InputOffset())
			key, err := d.Token()
			if err != nil {
				return nil, err
			}
			if !validJSONUnicode(b.source[start:int(d.InputOffset())]) {
				return nil, fail("parse", "invalidJson", "")
			}
			name, ok := key.(string)
			if !ok {
				return nil, fail("parse", "invalidJson", path)
			}
			p := domain.Pointer(path, name)
			if _, exists := result[name]; exists {
				return nil, fail("parse", "duplicateKey", p)
			}
			if err = b.collection(path, len(result)+1); err != nil {
				return nil, err
			}
			v, e := b.jsonValue(d, p, depth+1)
			if e != nil {
				return nil, e
			}
			result[name] = v
		}
		_, err = d.Token()
		return result, err
	case json.Delim('['):
		result := []any{}
		for d.More() {
			if err = b.collection(domain.Pointer(path, strconv.Itoa(len(result))), len(result)+1); err != nil {
				return nil, err
			}
			v, e := b.jsonValue(d, domain.Pointer(path, strconv.Itoa(len(result))), depth+1)
			if e != nil {
				return nil, e
			}
			result = append(result, v)
		}
		_, err = d.Token()
		return result, err
	default:
		if _, bad := token.(json.Delim); bad {
			return nil, fail("parse", "invalidJson", path)
		}
		if n, ok := token.(json.Number); ok {
			v, e := strconv.ParseFloat(string(n), 64)
			if e != nil || math.IsInf(v, 0) {
				return nil, fail("parse", "invalidJson", "")
			}
		}
		return token, nil
	}
}

// Reject isolated UTF-16 surrogate escapes before encoding/json can silently
// replace them with U+FFFD and lose the original invalid input.
func validJSONUnicode(source []byte) bool {
	inString := false
	for i := 0; i < len(source); i++ {
		if source[i] == '"' {
			inString = !inString
			continue
		}
		if !inString || source[i] != '\\' {
			continue
		}
		i++
		if i >= len(source) {
			return false
		}
		if source[i] != 'u' {
			continue
		}
		if i+4 >= len(source) {
			return false
		}
		v, e := strconv.ParseUint(string(source[i+1:i+5]), 16, 16)
		if e != nil {
			return false
		}
		i += 4
		if v >= 0xdc00 && v <= 0xdfff {
			return false
		}
		if v >= 0xd800 && v <= 0xdbff {
			if i+6 >= len(source) || source[i+1] != '\\' || source[i+2] != 'u' {
				return false
			}
			low, e := strconv.ParseUint(string(source[i+3:i+7]), 16, 16)
			if e != nil || low < 0xdc00 || low > 0xdfff {
				return false
			}
			i += 6
		}
	}
	return true
}

// YAML is read as syntax nodes, never generic YAML values: its implicit scalar
// coercion, merge behavior, and alias expansion are deliberately bypassed.
func (b *budget) yamlValue(n *yaml.Node, path string, depth int, key bool) (value any, err error) {
	defer func() {
		var d domain.Diagnostic
		if errors.As(err, &d) {
			if d.Line == nil {
				line, col := n.Line, n.Column
				d.Line = &line
				d.Column = &col
			}
			err = d
		}
	}()
	if n.Kind == yaml.AliasNode {
		return nil, fail("parse", "yamlAlias", path)
	}
	if n.Anchor != "" || n.Style&yaml.TaggedStyle != 0 {
		return nil, fail("parse", "yamlMetadata", path)
	}
	if !key {
		if err = b.count(path, depth); err != nil {
			return nil, err
		}
	}
	switch n.Kind {
	case yaml.MappingNode:
		if key {
			return nil, fail("parse", "nonStringKey", path)
		}
		result := map[string]any{}
		for i := 0; i < len(n.Content); i += 2 {
			k, e := b.yamlValue(n.Content[i], path, depth+1, true)
			if e != nil {
				return nil, e
			}
			name, ok := k.(string)
			if !ok {
				return nil, fail("parse", "nonStringKey", path)
			}
			p := domain.Pointer(path, name)
			if _, exists := result[name]; exists {
				d := domain.Problem("parse", "duplicateKey", p, "Duplicate object key.")
				line, column := n.Content[i].Line, n.Content[i].Column
				d.Line = &line
				d.Column = &column
				return nil, d
			}
			if e = b.collection(path, len(result)+1); e != nil {
				return nil, e
			}
			v, e := b.yamlValue(n.Content[i+1], p, depth+1, false)
			if e != nil {
				return nil, e
			}
			result[name] = v
		}
		return result, nil
	case yaml.SequenceNode:
		if key {
			return nil, fail("parse", "nonStringKey", path)
		}
		result := []any{}
		for i, child := range n.Content {
			if e := b.collection(path, i+1); e != nil {
				return nil, e
			}
			v, e := b.yamlValue(child, domain.Pointer(path, strconv.Itoa(i)), depth+1, false)
			if e != nil {
				return nil, e
			}
			result = append(result, v)
		}
		return result, nil
	case yaml.ScalarNode:
		s := n.Value
		if n.Style != 0 {
			return s, nil
		}
		if s == "<<" {
			return nil, fail("parse", "yamlMerge", path)
		}
		numeric := len(s) > 0 && strings.ContainsRune("0123456789+-.", rune(s[0]))
		if key && !numeric && s != "" && s != "true" && s != "false" && s != "null" {
			return s, nil
		}
		if s == "true" {
			return true, nil
		}
		if s == "false" {
			return false, nil
		}
		if s == "null" {
			return nil, nil
		}
		if numeric {
			dec := json.NewDecoder(strings.NewReader(s))
			dec.UseNumber()
			var v any
			if dec.Decode(&v) == nil {
				if _, ok := v.(json.Number); ok {
					n, finite := strconv.ParseFloat(s, 64)
					if finite == nil && !math.IsInf(n, 0) && dec.Decode(new(any)) == io.EOF {
						return v, nil
					}
				}
			}
		}
		ambiguous := s == "" || numeric
		for _, word := range []string{"yes", "no", "on", "off", "y", "n", "~", "true", "false", "null", ".inf", ".nan", "+.inf", "-.inf"} {
			if strings.ToLower(s) == word {
				ambiguous = true
			}
		}
		if ambiguous {
			return nil, fail("parse", "ambiguousScalar", path)
		}
		return s, nil
	default:
		return nil, fail("parse", "invalidYaml", path)
	}
}
