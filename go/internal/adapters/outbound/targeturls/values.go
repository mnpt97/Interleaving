package targeturls

import (
	"net/url"
	"strings"
)

func (Adapter) Encode(value string) string {
	var b strings.Builder
	const hex = "0123456789ABCDEF"
	for _, ch := range []byte(value) {
		if ch >= 'a' && ch <= 'z' || ch >= 'A' && ch <= 'Z' || ch >= '0' && ch <= '9' {
			b.WriteByte(ch)
		} else {
			b.WriteByte('%')
			b.WriteByte(hex[ch>>4])
			b.WriteByte(hex[ch&15])
		}
	}
	return b.String()
}
func (Adapter) HideQuery(value, replacement string) (string, []string) {
	u, err := url.Parse(value)
	if err != nil {
		return "[URL omitted]", nil
	}
	u.User = nil
	u.Fragment = ""
	secrets := []string{}
	if u.RawQuery != "" {
		pairs := []string{}
		for _, pair := range strings.Split(u.RawQuery, "&") {
			parts := strings.SplitN(pair, "=", 2)
			key, _ := url.QueryUnescape(parts[0])
			if len(parts) > 1 {
				decoded, _ := url.QueryUnescape(parts[1])
				secrets = append(secrets, parts[1], decoded)
			}
			pairs = append(pairs, url.QueryEscape(key)+"="+url.QueryEscape(replacement))
		}
		u.RawQuery = strings.Join(pairs, "&")
	}
	return u.String(), secrets
}
