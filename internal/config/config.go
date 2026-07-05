// Package config loads ahab configuration from the user config directory
// (~/.config/ahab/config.yaml) and the per-project directory (.ahab/config.yaml),
// with project values overriding user values.
package config

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

// ProjectDir is the per-project state directory, relative to the working directory.
const ProjectDir = ".ahab"

// Config holds all ahab settings. Zero values are replaced by defaults in Load.
type Config struct {
	// Provider selects the LLM provider implementation (only "anthropic" in v1).
	Provider string `yaml:"provider"`
	// Model is the model ID passed to the provider.
	Model string `yaml:"model"`
	// MaxTokens caps output tokens per model request.
	MaxTokens int `yaml:"maxTokens"`
	// KubeContext, when set, is passed to kubectl/helm as --context/--kube-context.
	KubeContext string `yaml:"kubeContext"`
	// Namespace, when set, is the default namespace for cluster commands.
	Namespace string `yaml:"namespace"`
	// Repos lists local paths to linked source repositories the agent may
	// explore during investigations.
	Repos []string `yaml:"repos"`

	// Auto enables unsupervised mode. Not persisted; set from the --auto flag.
	Auto bool `yaml:"-"`
	// WorkDir is the project root (where .ahab/ lives). Defaults to the CWD.
	WorkDir string `yaml:"-"`
}

// Defaults returns the built-in configuration.
func Defaults() *Config {
	return &Config{
		Provider:  "anthropic",
		Model:     "claude-opus-4-8",
		MaxTokens: 16000,
	}
}

// Load reads the user and project config files, if present, and merges them
// over the defaults. A missing file is not an error.
func Load(workDir string) (*Config, error) {
	cfg := Defaults()
	cfg.WorkDir = workDir

	if userDir, err := os.UserConfigDir(); err == nil {
		if err := mergeFile(cfg, filepath.Join(userDir, "ahab", "config.yaml")); err != nil {
			return nil, err
		}
	}
	if err := mergeFile(cfg, filepath.Join(workDir, ProjectDir, "config.yaml")); err != nil {
		return nil, err
	}
	return cfg, nil
}

func mergeFile(cfg *Config, path string) error {
	data, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(data, cfg); err != nil {
		return fmt.Errorf("parsing %s: %w", path, err)
	}
	return nil
}

// HistoryDir returns the session history directory, creating it if needed.
func (c *Config) HistoryDir() (string, error) {
	return c.ensureDir("history")
}

// ReportsDir returns the investigation reports directory, creating it if needed.
func (c *Config) ReportsDir() (string, error) {
	return c.ensureDir("reports")
}

func (c *Config) ensureDir(name string) (string, error) {
	dir := filepath.Join(c.WorkDir, ProjectDir, name)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}
