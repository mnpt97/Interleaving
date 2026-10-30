// Package targeturls matches the reference's WHATWG URL parsing without DNS or IO.
package targeturls

import (
	"strings"

	"github.com/nlnwa/whatwg-url/url"
	"interleaving/internal/domain"
)

type Adapter struct{}

var _ domain.TargetURLs = Adapter{}

// BaseURL returns no parser error text: URLs may contain attempted credentials.
// Checking the serialization also detects empty query/fragment delimiters, which
// Query/Fragment string accessors cannot distinguish from absent components.
func (Adapter) BaseURL(source string) (string, bool) {
	if len(source) > 8192 {
		return "", false
	}
	u, err := url.Parse(source)
	if err != nil {
		return "", false
	}
	canonical := u.Href(false)
	if (u.Scheme() != "http" && u.Scheme() != "https") || u.Hostname() == "" || u.Username() != "" || u.Password() != "" || strings.ContainsAny(canonical, "?#") {
		return "", false
	}
	return canonical, true
}
