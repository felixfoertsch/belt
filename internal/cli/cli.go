package cli

import (
	"errors"
	"flag"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"uberspace-cli/internal/config"
	"uberspace-cli/internal/httpapi"
	"uberspace-cli/internal/session"
)

type Command func(args []string) error

type CLI struct {
	Stdout *os.File
	Stderr *os.File
}

func New() *CLI {
	return &CLI{Stdout: os.Stdout, Stderr: os.Stderr}
}

func (c *CLI) Run(args []string) int {
	if len(args) < 1 {
		c.usage()
		return 2
	}
	switch args[0] {
	case "help", "-h", "--help":
		c.usage()
		return 0
	case "login":
		if err := c.login(args[1:]); err != nil {
			fmt.Fprintln(c.Stderr, "error:", err)
			return 1
		}
		return 0
	case "request":
		if err := c.request(args[1:]); err != nil {
			fmt.Fprintln(c.Stderr, "error:", err)
			return 1
		}
		return 0
	case "create-asteroid":
		if err := c.createAsteroid(args[1:]); err != nil {
			fmt.Fprintln(c.Stderr, "error:", err)
			return 1
		}
		return 0
	case "add-ssh-key":
		if err := c.addSSHKey(args[1:]); err != nil {
			fmt.Fprintln(c.Stderr, "error:", err)
			return 1
		}
		return 0
	default:
		fmt.Fprintln(c.Stderr, "unknown command:", args[0])
		c.usage()
		return 2
	}
}

func (c *CLI) usage() {
	fmt.Fprintln(c.Stdout, "uberspace-cli (experimental)")
	fmt.Fprintln(c.Stdout, "")
	fmt.Fprintln(c.Stdout, "Commands:")
	fmt.Fprintln(c.Stdout, "  login            Authenticate and store a session")
	fmt.Fprintln(c.Stdout, "  request          Execute a configured endpoint")
	fmt.Fprintln(c.Stdout, "  create-asteroid  Create a new asteroid")
	fmt.Fprintln(c.Stdout, "  add-ssh-key      Add an SSH public key")
	fmt.Fprintln(c.Stdout, "")
	fmt.Fprintln(c.Stdout, "Use 'command -h' for command help.")
}

func (c *CLI) login(args []string) error {
	fs := flag.NewFlagSet("login", flag.ContinueOnError)
	fs.SetOutput(c.Stderr)
	configPath := fs.String("config", "", "config file path")
	sessionPath := fs.String("session", "", "session file path")
	email := fs.String("email", "", "account email")
	password := fs.String("password", "", "account password")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *email == "" || *password == "" {
		return errors.New("email and password are required")
	}
	cfg, cfgPath, err := loadConfig(*configPath)
	if err != nil {
		return err
	}
	path, err := resolveSessionPath(*sessionPath)
	if err != nil {
		return err
	}

	ep, ok := cfg.Endpoints["login"]
	if !ok {
		return fmt.Errorf("login endpoint not found in %s", cfgPath)
	}

	client, err := httpapi.New(cfg.BaseURL, nil, nil)
	if err != nil {
		return err
	}
	vars := map[string]string{
		"email":    *email,
		"password": *password,
	}
	resp, body, err := client.DoEndpoint(ep, vars)
	if err != nil {
		return err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("login failed: %s: %s", resp.Status, strings.TrimSpace(string(body)))
	}
	csrf := client.CaptureCSRF(ep, resp, body)
	if err := session.Save(path, cfg.BaseURL, client.HTTP.Jar, csrf); err != nil {
		return err
	}
	fmt.Fprintln(c.Stdout, "session saved to", path)
	return nil
}

func (c *CLI) request(args []string) error {
	fs := flag.NewFlagSet("request", flag.ContinueOnError)
	fs.SetOutput(c.Stderr)
	configPath := fs.String("config", "", "config file path")
	sessionPath := fs.String("session", "", "session file path")
	endpoint := fs.String("endpoint", "", "endpoint name from config")
	varsFlag := fs.String("var", "", "template variables (key=value,comma-separated)")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *endpoint == "" {
		return errors.New("endpoint is required")
	}
	cfg, cfgPath, err := loadConfig(*configPath)
	if err != nil {
		return err
	}
	ep, ok := cfg.Endpoints[*endpoint]
	if !ok {
		return fmt.Errorf("endpoint %q not found in %s", *endpoint, cfgPath)
	}
	path, err := resolveSessionPath(*sessionPath)
	if err != nil {
		return err
	}
	var file *session.File
	var jar http.CookieJar
	if _, err := os.Stat(path); err == nil {
		f, j, err := session.Load(path)
		if err != nil {
			return err
		}
		file, jar = f, j
	}
	client, err := httpapi.New(cfg.BaseURL, jar, nil)
	if err != nil {
		return err
	}
	if file != nil {
		client.CSRF = file.CSRF
	}
	vars := parseVars(*varsFlag)
	resp, body, err := client.DoEndpoint(ep, vars)
	if err != nil {
		return err
	}
	if resp.StatusCode >= 400 {
		return fmt.Errorf("request failed: %s: %s", resp.Status, strings.TrimSpace(string(body)))
	}
	fmt.Fprintln(c.Stdout, string(body))
	return nil
}

func (c *CLI) createAsteroid(args []string) error {
	fs := flag.NewFlagSet("create-asteroid", flag.ContinueOnError)
	fs.SetOutput(c.Stderr)
	configPath := fs.String("config", "", "config file path")
	sessionPath := fs.String("session", "", "session file path")
	name := fs.String("name", "", "asteroid name")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *name == "" {
		return errors.New("name is required")
	}
	return c.callEndpoint("create_asteroid", map[string]string{"asteroid": *name}, *configPath, *sessionPath)
}

func (c *CLI) addSSHKey(args []string) error {
	fs := flag.NewFlagSet("add-ssh-key", flag.ContinueOnError)
	fs.SetOutput(c.Stderr)
	configPath := fs.String("config", "", "config file path")
	sessionPath := fs.String("session", "", "session file path")
	name := fs.String("name", "", "key name")
	publicKeyFile := fs.String("public-key-file", "", "path to public key file")
	publicKey := fs.String("public-key", "", "public key string")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *name == "" {
		return errors.New("name is required")
	}
	key, err := readPublicKey(*publicKeyFile, *publicKey)
	if err != nil {
		return err
	}
	vars := map[string]string{
		"key_name":   *name,
		"public_key": key,
	}
	return c.callEndpoint("add_ssh_key", vars, *configPath, *sessionPath)
}

func (c *CLI) callEndpoint(endpoint string, vars map[string]string, configPath string, sessionPath string) error {
	cfg, cfgPath, err := loadConfig(configPath)
	if err != nil {
		return err
	}
	ep, ok := cfg.Endpoints[endpoint]
	if !ok {
		return fmt.Errorf("endpoint %q not found in %s", endpoint, cfgPath)
	}
	path, err := resolveSessionPath(sessionPath)
	if err != nil {
		return err
	}
	file, jar, err := session.Load(path)
	if err != nil {
		return fmt.Errorf("session load failed (%s). Run login first", path)
	}
	client, err := httpapi.New(cfg.BaseURL, jar, nil)
	if err != nil {
		return err
	}
	client.CSRF = file.CSRF
	resp, body, err := client.DoEndpoint(ep, vars)
	if err != nil {
		return err
	}
	if resp.StatusCode >= 400 {
		return fmt.Errorf("request failed: %s: %s", resp.Status, strings.TrimSpace(string(body)))
	}
	fmt.Fprintln(c.Stdout, string(body))
	return nil
}

func loadConfig(path string) (*config.Config, string, error) {
	if path == "" {
		p, err := config.DefaultPath()
		if err != nil {
			return nil, "", err
		}
		path = p
	}
	cfg, err := config.Load(path)
	if err != nil {
		return nil, "", err
	}
	return cfg, path, nil
}

func resolveSessionPath(path string) (string, error) {
	if path != "" {
		return path, nil
	}
	return session.DefaultPath()
}

func parseVars(raw string) map[string]string {
	vars := map[string]string{}
	if raw == "" {
		return vars
	}
	pairs := strings.Split(raw, ",")
	for _, p := range pairs {
		parts := strings.SplitN(strings.TrimSpace(p), "=", 2)
		if len(parts) == 2 {
			vars[parts[0]] = parts[1]
		}
	}
	return vars
}

func readPublicKey(path string, literal string) (string, error) {
	if literal != "" {
		return strings.TrimSpace(literal), nil
	}
	if path == "" {
		path = filepath.Join(os.Getenv("HOME"), ".ssh", "id_ed25519.pub")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(data)), nil
}
