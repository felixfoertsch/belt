package config

import (
	"errors"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

type Config struct {
	BaseURL   string               `yaml:"base_url"`
	Endpoints map[string]Endpoint  `yaml:"endpoints"`
}

type Endpoint struct {
	Method  string            `yaml:"method"`
	Path    string            `yaml:"path"`
	Headers map[string]string `yaml:"headers"`
	Body    string            `yaml:"body"`
	CSRF    *CSRFConfig        `yaml:"csrf"`
}

type CSRFConfig struct {
	From        string `yaml:"from"`  // "cookie" or "header"
	Name        string `yaml:"name"`  // cookie or header name to read from response
	Header      string `yaml:"header"` // header to send on requests
}

func DefaultPath() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "uberspace-cli", "config.yaml"), nil
}

func Load(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var cfg Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, err
	}
	if cfg.BaseURL == "" {
		return nil, errors.New("base_url is required in config")
	}
	if len(cfg.Endpoints) == 0 {
		return nil, errors.New("endpoints are required in config")
	}
	return &cfg, nil
}
