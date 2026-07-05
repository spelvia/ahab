// Package factory constructs the configured llm.Provider. It owns the
// per-provider defaults (endpoint, credential env var, default model) so the
// rest of the program deals only in the neutral llm.Provider interface.
package factory

import (
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/spelvia/ahab/internal/config"
	"github.com/spelvia/ahab/internal/llm"
	"github.com/spelvia/ahab/internal/llm/anthropic"
	"github.com/spelvia/ahab/internal/llm/openaicompat"
)

// compatDefaults describes a known OpenAI-compatible provider.
type compatDefaults struct {
	baseURL string
	keyEnv  string
	model   string
	// maxCompletionTokens: OpenAI's current models require the
	// max_completion_tokens parameter; DeepSeek and Qwen still use max_tokens.
	maxCompletionTokens bool
}

var compatProviders = map[string]compatDefaults{
	"openai": {
		baseURL:             "https://api.openai.com/v1",
		keyEnv:              "OPENAI_API_KEY",
		model:               "gpt-5.5",
		maxCompletionTokens: true,
	},
	"deepseek": {
		baseURL: "https://api.deepseek.com/v1",
		keyEnv:  "DEEPSEEK_API_KEY",
		model:   "deepseek-v4-pro",
	},
	"qwen": {
		// Alibaba DashScope, OpenAI-compatible mode (international endpoint;
		// override baseURL in config for a regional endpoint).
		baseURL: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
		keyEnv:  "DASHSCOPE_API_KEY",
		model:   "qwen-max",
	},
}

const defaultAnthropicModel = "claude-opus-4-8"

// Providers lists the supported provider names.
func Providers() []string {
	names := []string{"anthropic", "openai-compatible"}
	for name := range compatProviders {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// New builds the provider selected by cfg.Provider. Config may override the
// model, endpoint (baseURL), and credential env var (apiKeyEnv) per provider.
func New(cfg *config.Config) (llm.Provider, error) {
	name := strings.ToLower(strings.TrimSpace(cfg.Provider))
	if name == "" {
		name = "anthropic"
	}

	if name == "anthropic" {
		model := cfg.Model
		if model == "" {
			model = defaultAnthropicModel
		}
		return anthropic.New(model), nil
	}

	var d compatDefaults
	if name == "openai-compatible" {
		// Bring-your-own endpoint (vLLM, Ollama, a gateway, ...): everything
		// must come from config.
		if cfg.BaseURL == "" || cfg.APIKeyEnv == "" || cfg.Model == "" {
			return nil, fmt.Errorf("provider openai-compatible requires baseURL, apiKeyEnv, and model in the config")
		}
	} else if known, ok := compatProviders[name]; ok {
		d = known
	} else {
		return nil, fmt.Errorf("unknown provider %q (supported: %s)", cfg.Provider, strings.Join(Providers(), ", "))
	}

	baseURL := cfg.BaseURL
	if baseURL == "" {
		baseURL = d.baseURL
	}
	keyEnv := cfg.APIKeyEnv
	if keyEnv == "" {
		keyEnv = d.keyEnv
	}
	model := cfg.Model
	if model == "" {
		model = d.model
	}
	apiKey := os.Getenv(keyEnv)
	if apiKey == "" {
		return nil, fmt.Errorf("provider %s: environment variable %s is not set", name, keyEnv)
	}

	return openaicompat.New(openaicompat.Config{
		BaseURL:                baseURL,
		APIKey:                 apiKey,
		Model:                  model,
		UseMaxCompletionTokens: d.maxCompletionTokens,
	}), nil
}
