// Package config reads the Lentera server settings from environment variables.
package config

import (
	"errors"
	"fmt"
	"os"
)

// LocalDatabaseURL points at the PostgreSQL container from compose.dev.yml.
const LocalDatabaseURL = "postgres://lentera:lentera-dev@127.0.0.1:54330/lentera?sslmode=disable"

// minimumSecretLength keeps LOGIN_CODE_SECRET long enough to resist guessing.
const minimumSecretLength = 32

// Config holds the settings the server needs at startup.
type Config struct {
	Environment     string
	HTTPAddress     string
	DatabaseURL     string
	LoginCodeSecret string
	ResendAPIKey    string
	MailFrom        string
}

// Load reads the configuration. Outside APP_ENV=local, DATABASE_URL and LOGIN_CODE_SECRET are
// required; production also requires RESEND_API_KEY. MAIL_FROM is required with RESEND_API_KEY.
func Load() (Config, error) {
	cfg := Config{
		Environment:     valueOrDefault("APP_ENV", "local"),
		HTTPAddress:     valueOrDefault("HTTP_ADDR", "127.0.0.1:8310"),
		DatabaseURL:     os.Getenv("DATABASE_URL"),
		LoginCodeSecret: os.Getenv("LOGIN_CODE_SECRET"),
		ResendAPIKey:    os.Getenv("RESEND_API_KEY"),
		MailFrom:        os.Getenv("MAIL_FROM"),
	}

	switch cfg.Environment {
	case "local", "staging", "production":
	default:
		return Config{}, fmt.Errorf("APP_ENV %q tidak dikenal; pakai local, staging, atau production", cfg.Environment)
	}
	local := cfg.Environment == "local"

	if cfg.DatabaseURL == "" {
		if !local {
			return Config{}, errors.New("DATABASE_URL wajib diisi jika APP_ENV bukan local")
		}
		cfg.DatabaseURL = LocalDatabaseURL
	}

	if cfg.LoginCodeSecret == "" && !local {
		return Config{}, errors.New("LOGIN_CODE_SECRET wajib diisi jika APP_ENV bukan local")
	}
	if cfg.LoginCodeSecret != "" && len(cfg.LoginCodeSecret) < minimumSecretLength {
		return Config{}, fmt.Errorf("LOGIN_CODE_SECRET minimal %d karakter", minimumSecretLength)
	}

	if cfg.ResendAPIKey == "" && cfg.Environment == "production" {
		return Config{}, errors.New("RESEND_API_KEY wajib diisi di production agar kode masuk benar-benar terkirim")
	}
	if cfg.ResendAPIKey != "" && cfg.MailFrom == "" {
		return Config{}, errors.New("MAIL_FROM wajib diisi jika RESEND_API_KEY diisi, misalnya: Lentera <masuk@domain-anda>")
	}

	return cfg, nil
}

func valueOrDefault(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}

	return fallback
}
