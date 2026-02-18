package httpapi

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"strings"
	"text/template"
	"time"

	"uberspace-cli/internal/config"
	"uberspace-cli/internal/session"
)

type Client struct {
	BaseURL string
	HTTP    *http.Client
	CSRF    *session.CSRFToken
}

func New(baseURL string, jar http.CookieJar, csrf *session.CSRFToken) (*Client, error) {
	if baseURL == "" {
		return nil, errors.New("base_url is required")
	}
	if jar == nil {
		jar, _ = cookiejar.New(nil)
	}
	return &Client{
		BaseURL: strings.TrimRight(baseURL, "/"),
		HTTP: &http.Client{
			Timeout: 30 * time.Second,
			Jar:     jar,
		},
		CSRF: csrf,
	}, nil
}

func (c *Client) DoEndpoint(ep config.Endpoint, vars map[string]string) (*http.Response, []byte, error) {
	body, err := renderTemplate(ep.Body, vars)
	if err != nil {
		return nil, nil, err
	}

	urlStr := c.BaseURL + ep.Path
	req, err := http.NewRequest(ep.Method, urlStr, strings.NewReader(body))
	if err != nil {
		return nil, nil, err
	}

	for k, v := range ep.Headers {
		req.Header.Set(k, v)
	}
	if c.CSRF != nil && c.CSRF.Header != "" && c.CSRF.Value != "" {
		req.Header.Set(c.CSRF.Header, c.CSRF.Value)
	}

	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, nil, err
	}
	defer resp.Body.Close()
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return resp, nil, err
	}
	return resp, respBody, nil
}

func (c *Client) CaptureCSRF(ep config.Endpoint, resp *http.Response, body []byte) *session.CSRFToken {
	if ep.CSRF == nil {
		return nil
	}
	cfg := ep.CSRF
	switch strings.ToLower(cfg.From) {
	case "cookie":
		u, err := url.Parse(c.BaseURL)
		if err != nil {
			return nil
		}
		cookies := c.HTTP.Jar.Cookies(u)
		for _, c := range cookies {
			if c.Name == cfg.Name {
				return &session.CSRFToken{Header: cfg.Header, Value: c.Value}
			}
		}
	case "header":
		val := resp.Header.Get(cfg.Name)
		if val != "" {
			return &session.CSRFToken{Header: cfg.Header, Value: val}
		}
	}
	_ = body
	return nil
}

func renderTemplate(tpl string, vars map[string]string) (string, error) {
	if tpl == "" {
		return "", nil
	}
	t, err := template.New("body").Option("missingkey=error").Parse(tpl)
	if err != nil {
		return "", fmt.Errorf("invalid template: %w", err)
	}
	var buf bytes.Buffer
	if err := t.Execute(&buf, vars); err != nil {
		return "", fmt.Errorf("template render failed: %w", err)
	}
	return buf.String(), nil
}
