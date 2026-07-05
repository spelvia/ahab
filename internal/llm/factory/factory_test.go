package factory

import (
	"strings"
	"testing"

	"github.com/spelvia/ahab/internal/config"
)

func TestAnthropicDefault(t *testing.T) {
	for _, provider := range []string{"", "anthropic", "Anthropic"} {
		cfg := config.Defaults()
		cfg.Provider = provider
		if _, err := New(cfg); err != nil {
			t.Errorf("provider %q: %v", provider, err)
		}
	}
}

func TestCompatProvidersRequireKey(t *testing.T) {
	for name, keyEnv := range map[string]string{
		"openai":   "OPENAI_API_KEY",
		"deepseek": "DEEPSEEK_API_KEY",
		"qwen":     "DASHSCOPE_API_KEY",
	} {
		t.Run(name, func(t *testing.T) {
			cfg := config.Defaults()
			cfg.Provider = name

			t.Setenv(keyEnv, "")
			if _, err := New(cfg); err == nil || !strings.Contains(err.Error(), keyEnv) {
				t.Fatalf("missing key not reported: %v", err)
			}

			t.Setenv(keyEnv, "sk-test")
			if _, err := New(cfg); err != nil {
				t.Fatalf("with key set: %v", err)
			}
		})
	}
}

func TestAPIKeyEnvOverride(t *testing.T) {
	cfg := config.Defaults()
	cfg.Provider = "deepseek"
	cfg.APIKeyEnv = "MY_CUSTOM_KEY"
	t.Setenv("MY_CUSTOM_KEY", "sk-custom")
	if _, err := New(cfg); err != nil {
		t.Fatal(err)
	}
}

func TestOpenAICompatibleRequiresFullConfig(t *testing.T) {
	cfg := config.Defaults()
	cfg.Provider = "openai-compatible"
	if _, err := New(cfg); err == nil || !strings.Contains(err.Error(), "requires baseURL") {
		t.Fatalf("incomplete config not reported: %v", err)
	}

	cfg.BaseURL = "http://localhost:11434/v1"
	cfg.APIKeyEnv = "LOCAL_KEY"
	cfg.Model = "llama3"
	t.Setenv("LOCAL_KEY", "x")
	if _, err := New(cfg); err != nil {
		t.Fatal(err)
	}
}

func TestUnknownProvider(t *testing.T) {
	cfg := config.Defaults()
	cfg.Provider = "grok"
	_, err := New(cfg)
	if err == nil || !strings.Contains(err.Error(), "unknown provider") {
		t.Fatalf("err = %v", err)
	}
	for _, want := range []string{"anthropic", "deepseek", "openai", "qwen"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("error should list %s: %v", want, err)
		}
	}
}
