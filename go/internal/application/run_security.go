package application

import (
	"encoding/json"
	"interleaving/internal/domain"
	"sort"
	"strings"
	"unicode/utf8"
)

// security owns all public projection policy. Raw provider/transport values are
// learned here before an event, graph, snapshot or error can leave the core.
type security struct {
	secrets      map[string]bool
	bytes        int
	exhausted    bool
	urls         URLValues
	fields       []string
	conservative bool
}

func (s *security) remember(value string) {
	pending := []string{value}
	for len(pending) > 0 {
		v := pending[len(pending)-1]
		pending = pending[:len(pending)-1]
		if v == "" || s.secrets[v] || s.exhausted {
			continue
		}
		escaped, _ := jsonWire(v)
		encoded := s.urls.Encode(v)
		lower := []byte(encoded)
		for i := 0; i+2 < len(lower); i++ {
			if lower[i] == '%' {
				lower[i+1] = byte(strings.ToLower(string(lower[i+1]))[0])
				lower[i+2] = byte(strings.ToLower(string(lower[i+2]))[0])
				i += 2
			}
		}
		for _, form := range []string{v, encoded, string(lower), string(escaped[1 : len(escaped)-1])} {
			if s.secrets[form] {
				continue
			}
			s.bytes += len(form)
			if s.bytes > 1048576 || len(s.secrets) >= 4096 {
				s.exhausted = true
				s.secrets = nil
				return
			}
			s.secrets[form] = true
		}
		if scheme, token, ok := strings.Cut(v, " "); ok && (strings.EqualFold(scheme, "bearer") || strings.EqualFold(scheme, "basic")) {
			pending = append(pending, token)
		}
		for _, part := range strings.Split(v, ";") {
			if _, v, ok := strings.Cut(part, "="); ok {
				pending = append(pending, strings.TrimSpace(v))
			}
		}
	}
}
func (s *security) rememberValue(value any) {
	switch v := value.(type) {
	case string:
		s.remember(v)
	case map[string]any:
		for _, v := range v {
			s.rememberValue(v)
		}
	case []any:
		for _, v := range v {
			s.rememberValue(v)
		}
	case json.Number, bool, int64, int:
		data, _ := json.Marshal(v)
		s.remember(string(data))
	}
}
func normalized(value string) string {
	var b strings.Builder
	for _, ch := range strings.ToLower(value) {
		if ch >= 'a' && ch <= 'z' || ch >= '0' && ch <= '9' {
			b.WriteRune(ch)
		}
	}
	return b.String()
}
func (s *security) sensitive(key string) bool {
	k := normalized(key)
	for _, part := range append([]string{"authorization", "cookie", "password", "passwd", "secret", "token", "apikey", "privatekey", "credential", "session"}, s.fields...) {
		if strings.Contains(k, normalized(part)) {
			return true
		}
	}
	return false
}
func (s *security) text(value string) string {
	if s.exhausted {
		return "[REDACTED]"
	}
	if len(s.secrets) == 0 {
		return value
	}
	var b strings.Builder
	for at := 0; at < len(value); {
		longest := ""
		for secret := range s.secrets {
			if len(secret) > len(longest) && strings.HasPrefix(value[at:], secret) {
				longest = secret
			}
		}
		if longest != "" {
			b.WriteString("[REDACTED]")
			at += len(longest)
		} else {
			_, n := utf8.DecodeRuneInString(value[at:])
			b.WriteString(value[at : at+n])
			at += n
		}
	}
	return b.String()
}
func (s *security) learn(value any) {
	switch v := value.(type) {
	case map[string]any:
		for k, v := range v {
			if s.sensitive(k) {
				s.rememberValue(v)
			}
			s.learn(v)
		}
	case []any:
		for _, v := range v {
			s.learn(v)
		}
	}
}
func (s *security) clean(value any) any {
	if s.exhausted {
		return "[Preview omitted: redaction budget exceeded]"
	}
	switch v := value.(type) {
	case string:
		return s.text(v)
	case map[string]any:
		r := map[string]any{}
		for k, v := range v {
			if s.sensitive(k) {
				r[s.text(k)] = "[REDACTED]"
			} else {
				r[s.text(k)] = s.clean(v)
			}
		}
		return r
	case []any:
		r := []any{}
		for _, v := range v {
			r = append(r, s.clean(v))
		}
		return r
	default:
		b, _ := json.Marshal(v)
		if s.secrets[string(b)] {
			return "[REDACTED]"
		}
		return v
	}
}
func (s *security) url(value string) string {
	safe, secrets := s.urls.HideQuery(value, "[REDACTED]")
	for _, v := range secrets {
		s.remember(v)
	}
	return s.text(safe)
}
func (s *security) headers(h map[string][]string) any {
	for k, vs := range h {
		if s.sensitive(k) {
			for _, v := range vs {
				s.remember(v)
			}
		}
	}
	if s.conservative || s.exhausted {
		return map[string]any{"[omitted]": "Sensitive authentication header values omitted"}
	}
	result := map[string]any{}
	keys := orderedKeys(h)
	size := 0
	for i, k := range keys {
		if i >= 64 {
			break
		}
		v := ""
		if len(h[k]) > 0 {
			v = h[k][len(h[k])-1]
		}
		if s.sensitive(k) {
			v = "[REDACTED]"
		} else if strings.Contains("|location|referer|refresh|link|", "|"+strings.ToLower(k)+"|") {
			v = "[URL header omitted]"
		} else {
			v = s.text(v)
		}
		size += len(k) + len(v)
		if size > 8192 {
			result["[omitted]"] = "Header preview limit reached"
			break
		}
		result[s.text(strings.ToLower(k))] = v
	}
	return result
}
func omittedBody(bytes int, truncated bool, reason string) any {
	return map[string]any{"value": nil, "bytes": bytes, "truncated": truncated, "omitted": true, "reason": reason}
}
func (s *security) body(value any, decoded bool, size int, truncated bool, limit int) any {
	if s.conservative {
		return omittedBody(size, truncated, "Sensitive authentication payload omitted")
	}
	clean := s.clean(value)
	data, _ := jsonWire(clean)
	if !decoded || truncated || len(data) > limit {
		return omittedBody(size, truncated, "Only complete JSON bodies within the preview limit are shown")
	}
	return map[string]any{"value": clean, "bytes": size, "truncated": false, "omitted": false}
}
func orderedKeys[V any](m map[string]V) []string {
	keys := []string{}
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
func (s *security) learnDocument(doc *domain.Document) {
	// Learn authored values, not codec-provided defaults such as "Bearer ".
	// Otherwise an omitted default would redact unrelated graph labels.
	value, err := admittedWire(doc)
	if err != nil {
		s.exhausted = true
		return
	}
	delete(value, "identities")
	var walk func(any)
	walk = func(value any) {
		switch v := value.(type) {
		case map[string]any:
			if v["source"] == "literal" {
				s.rememberValue(v["value"])
				return
			}
			for key, child := range v {
				if key == "default" || key == "prefix" || key == "headerPrefix" {
					s.rememberValue(child)
				}
				if key == "headers" {
					s.learn(child)
				}
				walk(child)
			}
			if v["kind"] == "text" {
				s.rememberValue(v["value"])
			}
			if v["kind"] == "json" {
				s.learn(v["value"])
			}
		case []any:
			for _, child := range v {
				walk(child)
			}
		}
	}
	walk(value)
	for _, value := range doc.Environment.Variables {
		s.rememberValue(value)
	}
}
