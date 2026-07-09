package config

import (
	"os"
	"path/filepath"

	"github.com/pelletier/go-toml/v2"
)

// Config holds user and environment configuration.
// Precedence (highest wins): CLI flags > env > config file > defaults.
type Config struct {
	// Model selection
	Model string `toml:"model"`

	// Provider / keys
	APIKey  string `toml:"api_key"`  // generic
	BaseURL string `toml:"base_url"` // OpenAI-compatible endpoint

	// XAI / Grok specific (first-class)
	XAIAPIKey string `toml:"xai_api_key"`

	// Behavior
	Yolo         bool   `toml:"yolo"`
	Permission   string `toml:"permission"` // default | dontAsk | acceptEdits | bypass
	MaxTurns     int    `toml:"max_turns"`

	// UI
	VimMode      bool `toml:"vim_mode"`
	CompactMode  bool `toml:"compact_mode"`

	// Paths
	SessionDir string `toml:"session_dir"`

	// Raw file path we loaded from (for diagnostics)
	ConfigPath string `toml:"-"`
}

// DefaultConfig returns a sane starting point.
func DefaultConfig() *Config {
	home, _ := os.UserHomeDir()
	return &Config{
		Model:       getEnv("ZENCODE_MODEL", "grok-3"), // good default when XAI key present
		APIKey:      os.Getenv("ZENCODE_API_KEY"),
		BaseURL:     os.Getenv("ZENCODE_BASE_URL"),
		XAIAPIKey:   os.Getenv("XAI_API_KEY"),
		Yolo:        false,
		Permission:  "default",
		MaxTurns:    40,
		VimMode:     false,
		CompactMode: false,
		SessionDir:  filepath.Join(home, ".zencode", "sessions"),
	}
}

// Load reads config file (if present) then overlays environment.
// It does NOT yet apply CLI flags (caller should do that after).
func Load() (*Config, error) {
	cfg := DefaultConfig()

	// Locate config file (XDG or ~/.zencode/config.toml)
	cfgPath := locateConfigFile()
	cfg.ConfigPath = cfgPath

	if cfgPath != "" {
		data, err := os.ReadFile(cfgPath)
		if err == nil {
			var fileCfg Config
			if err := toml.Unmarshal(data, &fileCfg); err == nil {
				mergeNonZero(cfg, &fileCfg)
			}
		}
	}

	// Env overlays (already seeded some in Default, but re-apply for clarity)
	if v := os.Getenv("XAI_API_KEY"); v != "" {
		cfg.XAIAPIKey = v
	}
	if v := os.Getenv("ZENCODE_API_KEY"); v != "" {
		cfg.APIKey = v
	}
	if v := os.Getenv("ZENCODE_BASE_URL"); v != "" {
		cfg.BaseURL = v
	}
	if v := os.Getenv("ZENCODE_MODEL"); v != "" {
		cfg.Model = v
	}

	// Smart defaults for Grok users
	if cfg.BaseURL == "" && cfg.XAIAPIKey != "" {
		cfg.BaseURL = "https://api.x.ai/v1"
	}
	if cfg.APIKey == "" && cfg.XAIAPIKey != "" {
		cfg.APIKey = cfg.XAIAPIKey
	}
	if cfg.Model == "" {
		cfg.Model = "grok-3"
	}

	return cfg, nil
}

// EffectiveKey returns the API key we should actually send.
func (c *Config) EffectiveKey() string {
	if c.APIKey != "" {
		return c.APIKey
	}
	return c.XAIAPIKey
}

// EffectiveBaseURL returns the base we should use for the OpenAI client.
func (c *Config) EffectiveBaseURL() string {
	if c.BaseURL != "" {
		return c.BaseURL
	}
	// Sensible fallback (OpenAI)
	return "https://api.openai.com/v1"
}

// locateConfigFile walks standard locations.
func locateConfigFile() string {
	// $XDG_CONFIG_HOME/zencode/config.toml
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		p := filepath.Join(xdg, "zencode", "config.toml")
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}

	// ~/.config/zencode/config.toml
	p := filepath.Join(home, ".config", "zencode", "config.toml")
	if _, err := os.Stat(p); err == nil {
		return p
	}

	// ~/.zencode/config.toml (our own convention)
	p = filepath.Join(home, ".zencode", "config.toml")
	if _, err := os.Stat(p); err == nil {
		return p
	}
	return ""
}

func mergeNonZero(dst, src *Config) {
	if src.Model != "" {
		dst.Model = src.Model
	}
	if src.APIKey != "" {
		dst.APIKey = src.APIKey
	}
	if src.BaseURL != "" {
		dst.BaseURL = src.BaseURL
	}
	if src.XAIAPIKey != "" {
		dst.XAIAPIKey = src.XAIAPIKey
	}
	if src.Permission != "" {
		dst.Permission = src.Permission
	}
	if src.SessionDir != "" {
		dst.SessionDir = src.SessionDir
	}
	if src.MaxTurns > 0 {
		dst.MaxTurns = src.MaxTurns
	}
	dst.Yolo = dst.Yolo || src.Yolo
	dst.VimMode = dst.VimMode || src.VimMode
	dst.CompactMode = dst.CompactMode || src.CompactMode
}

func getEnv(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}
