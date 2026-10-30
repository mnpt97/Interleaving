// Package httpclient owns net/http clients, isolated cookie jars and wire errors.
package httpclient

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"sort"
	"strings"
	"time"

	whatwg "github.com/nlnwa/whatwg-url/url"
	"golang.org/x/net/publicsuffix"
	"interleaving/internal/application"
	"interleaving/internal/domain"
)

type Adapter struct{}
type session struct {
	client    *http.Client
	transport *http.Transport
	profile   domain.ClientProfile
	options   application.RunOptions
	headers   map[string]string
	removed   map[string]bool
}

func config(message string) *application.Failure {
	return &application.Failure{Kind: "configuration", Message: message}
}
func (Adapter) New(profile domain.ClientProfile, options application.RunOptions) (application.Session, *application.Failure) {
	if options.MaxResponseBytes < 1 || options.MaxResponseBytes > 1048576 || options.MaxRequestBytes < 1 || options.MaxRequestBytes > 1048576 {
		return nil, config("HTTP byte limits must be between 1 and 1048576")
	}
	if profile.TimeoutMS == 0 || profile.TimeoutMS > 300000 {
		return nil, config("Timeout must be between 1 and 300000 ms")
	}
	transport := http.DefaultTransport.(*http.Transport).Clone()
	headers := make(map[string]string, len(profile.Headers))
	for name, value := range profile.Headers {
		headers[name] = value
	}
	profile.Headers = headers
	transport.MaxResponseHeaderBytes = 65536
	transport.DisableCompression = true
	// No automatic replay: do not supply GetBody and do not follow redirects.
	s := &session{transport: transport, profile: profile, options: options, client: &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}}
	if err := s.newJar(); err != nil {
		return nil, err
	}
	return s, nil
}
func (s *session) Reset() *application.Failure {
	// Fresh login keeps ordinary profile defaults, but must not inherit an old
	// identity through either explicit auth headers or transport cookies.
	for name := range s.profile.Headers {
		switch strings.ToLower(name) {
		case "authorization", "proxy-authorization", "cookie":
			delete(s.profile.Headers, name)
		}
	}
	return s.newJar()
}
func (s *session) newJar() *application.Failure {
	jar, err := cookiejar.New(&cookiejar.Options{PublicSuffixList: publicsuffix.List})
	if err != nil {
		return config("Cookie session initialization failed")
	}
	s.client.Jar = jar
	s.headers = map[string]string{}
	s.removed = map[string]bool{}
	return nil
}
func validHeader(name, value string) bool {
	if !domain.HeaderName(name) {
		return false
	}
	for _, b := range []byte(value) {
		if b < 32 && b != '\t' || b == 127 {
			return false
		}
	}
	return true
}
func (s *session) UpdateHeaders(set map[string]string, remove []string) *application.Failure {
	if len(set) == 0 && len(remove) == 0 {
		return config("Session update is empty")
	}
	next := map[string]string{}
	removed := map[string]bool{}
	for k, v := range s.headers {
		next[k] = v
	}
	for k, v := range s.removed {
		removed[k] = v
	}
	seen := map[string]bool{}
	for k, v := range set {
		key := strings.ToLower(k)
		if !validHeader(k, v) || len(k) > 128 || len(v) > 8192 || seen[key] {
			return config("Invalid or oversized session header value")
		}
		seen[key] = true
		next[key] = v
		delete(removed, key)
	}
	for _, k := range remove {
		key := strings.ToLower(k)
		if !validHeader(k, "") || len(k) > 128 || seen[key] {
			return config("Invalid session header operation")
		}
		seen[key] = true
		delete(next, key)
		removed[key] = true
	}
	if len(next) > 32768 {
		return config("HTTP header storage capacity exceeded")
	}
	s.headers = next
	s.removed = removed
	return nil
}
func (s *session) Prepare(input application.HTTPInput) (application.Request, *application.Failure) {
	ms := input.TimeoutMS
	if ms == 0 {
		ms = s.profile.TimeoutMS
	}
	if ms > 300000 || ms == 0 {
		return application.Request{}, config("Timeout must be between 1 and 300000 ms")
	}
	path := input.Path
	if !strings.HasPrefix(path, "/") || strings.HasPrefix(path, "//") || strings.ContainsAny(path, "\r\n#") || len(path) > s.options.MaxRequestBytes {
		return application.Request{}, config("Invalid or oversized request path")
	}
	base, err := whatwg.Parse(input.BaseURL)
	if err != nil {
		return application.Request{}, config("Invalid base URL")
	}
	base.SetPathname(strings.TrimRight(base.Pathname(), "/") + "/")
	target, err := base.Parse(path[1:])
	if err != nil || target.Scheme() != base.Scheme() || target.Host() != base.Host() || !strings.HasPrefix(target.Pathname(), base.Pathname()) {
		return application.Request{}, config("Request path must preserve its base URL origin and path prefix")
	}
	if len(target.Pathname())+len(target.Search()) > s.options.MaxRequestBytes {
		return application.Request{}, config("Encoded request path exceeds the byte limit")
	}
	if len(input.Body) > s.options.MaxRequestBytes {
		return application.Request{}, config("Request body exceeds the byte limit")
	}
	headers := map[string]string{}
	for _, k := range headerKeys(s.profile.Headers) {
		v := s.profile.Headers[k]
		headers[strings.ToLower(k)] = v
	}
	for k := range s.removed {
		delete(headers, k)
	}
	for k, v := range s.headers {
		headers[k] = v
	}
	for _, k := range headerKeys(input.Headers) {
		v := input.Headers[k]
		headers[strings.ToLower(k)] = v
	}
	if input.ContentType != "" {
		if _, ok := headers["content-type"]; !ok {
			headers["content-type"] = input.ContentType
		}
	}
	for k, v := range headers {
		if !validHeader(k, v) {
			return application.Request{}, config("Invalid request header")
		}
	}
	return application.Request{URL: target.Href(false), Method: input.Method, Headers: headers, Body: input.Body, TimeoutMS: ms, JSONBody: input.JSONBody}, nil
}

func headerKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
func (s *session) Do(ctx context.Context, request application.Request) (application.Response, *application.Failure) {
	call, cancel := context.WithTimeout(ctx, time.Duration(request.TimeoutMS)*time.Millisecond)
	defer cancel()
	req, err := http.NewRequestWithContext(call, request.Method, request.URL, io.NopCloser(bytes.NewReader(request.Body)))
	if err != nil {
		return application.Response{}, config("Request could not be constructed")
	}
	req.ContentLength = int64(len(request.Body))
	// Preserve the wire contract: no implicit user agent or gzip negotiation.
	req.Header.Set("User-Agent", "")
	if _, explicit := request.Headers["accept"]; !explicit {
		req.Header.Set("Accept", "*/*")
	}
	for k, v := range request.Headers {
		req.Header.Set(k, v)
	}
	response, err := s.client.Do(req)
	if err != nil {
		return application.Response{}, wireError(ctx, call, err)
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, int64(s.options.MaxResponseBytes)+1))
	if err != nil {
		failure := wireError(ctx, call, err)
		if failure.Kind == "transport" || failure.Kind == "connect" {
			failure = &application.Failure{Kind: "body", Message: "Response body could not be read"}
		}
		return application.Response{}, failure
	}
	truncated := len(data) > s.options.MaxResponseBytes
	if truncated {
		data = data[:s.options.MaxResponseBytes]
	}
	headers := response.Header.Clone()
	// net/http consumes the HTTP/1 Connection: close field into Response.Close.
	if response.ProtoMajor == 1 && response.Close && headers.Get("Connection") == "" {
		headers.Set("Connection", "close")
	}
	return application.Response{Status: uint16(response.StatusCode), Headers: map[string][]string(headers), Body: data, Truncated: truncated}, nil
}
func wireError(parent, call context.Context, err error) *application.Failure {
	kind := "transport"
	var dns *net.DNSError
	var unknown x509.UnknownAuthorityError
	var cert *tls.CertificateVerificationError
	var operation *net.OpError
	switch {
	case parent.Err() != nil:
		kind = "cancelled"
	case call.Err() == context.DeadlineExceeded:
		kind = "timeout"
	case errors.As(err, &dns):
		kind = "dns"
	case errors.As(err, &unknown) || errors.As(err, &cert):
		kind = "tls"
	case errors.As(err, &operation) && operation.Op == "dial":
		kind = "connect"
	}
	message := map[string]string{"dns": "DNS resolution failed", "tls": "TLS negotiation or certificate verification failed", "connect": "Connection failed", "timeout": "Request timed out", "cancelled": "Run cancelled", "transport": "HTTP transport failed"}[kind]
	return &application.Failure{Kind: kind, Message: message}
}
func (s *session) HasResponseCookies(target string, response application.Response) bool {
	u, err := url.Parse(target)
	if err != nil {
		return false
	}
	h := http.Header(response.Headers)
	received := (&http.Response{Header: h}).Cookies()
	for _, cookie := range received {
		target := *u
		if strings.HasPrefix(cookie.Path, "/") {
			target.Path = cookie.Path
			target.RawPath = ""
		}
		for _, active := range s.client.Jar.Cookies(&target) {
			if cookie.Name == active.Name && cookie.Value == active.Value && active.Value != "" {
				return true
			}
		}
	}
	return false
}
func (s *session) Close() { s.transport.CloseIdleConnections() }
