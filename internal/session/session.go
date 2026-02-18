package session

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"os"
	"path/filepath"
	"time"
)

type File struct {
	BaseURL string     `json:"base_url"`
	Cookies []Cookie   `json:"cookies"`
	CSRF    *CSRFToken `json:"csrf,omitempty"`
}

type Cookie struct {
	Name     string `json:"name"`
	Value    string `json:"value"`
	Domain   string `json:"domain"`
	Path     string `json:"path"`
	Expires  int64  `json:"expires"`
	Secure   bool   `json:"secure"`
	HttpOnly bool   `json:"http_only"`
}

type CSRFToken struct {
	Header string `json:"header"`
	Value  string `json:"value"`
}

func DefaultPath() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "uberspace-cli", "session.json"), nil
}

func Load(path string) (*File, http.CookieJar, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, err
	}
	var f File
	if err := json.Unmarshal(data, &f); err != nil {
		return nil, nil, err
	}
	if f.BaseURL == "" {
		return nil, nil, errors.New("session missing base_url")
	}
	u, err := url.Parse(f.BaseURL)
	if err != nil {
		return nil, nil, err
	}
	jar, err := cookiejar.New(nil)
	if err != nil {
		return nil, nil, err
	}
	cookies := make([]*http.Cookie, 0, len(f.Cookies))
	for _, c := range f.Cookies {
		cookies = append(cookies, &http.Cookie{
			Name:     c.Name,
			Value:    c.Value,
			Domain:   c.Domain,
			Path:     c.Path,
			Expires:  unixToTime(c.Expires),
			Secure:   c.Secure,
			HttpOnly: c.HttpOnly,
		})
	}
	jar.SetCookies(u, cookies)
	return &f, jar, nil
}

func Save(path string, baseURL string, jar http.CookieJar, csrf *CSRFToken) error {
	u, err := url.Parse(baseURL)
	if err != nil {
		return err
	}
	cookies := jar.Cookies(u)
	out := File{BaseURL: baseURL, CSRF: csrf}
	for _, c := range cookies {
		expires := int64(0)
		if !c.Expires.IsZero() {
			expires = c.Expires.Unix()
		}
		out.Cookies = append(out.Cookies, Cookie{
			Name:     c.Name,
			Value:    c.Value,
			Domain:   c.Domain,
			Path:     c.Path,
			Expires:  expires,
			Secure:   c.Secure,
			HttpOnly: c.HttpOnly,
		})
	}
	data, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o600)
}

func unixToTime(v int64) (t time.Time) {
	if v == 0 {
		return time.Time{}
	}
	return time.Unix(v, 0)
}
