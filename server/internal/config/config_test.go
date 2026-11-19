package config

import (
	"strings"
	"testing"
)

func setEnv(t *testing.T, values map[string]string) {
	t.Helper()
	for _, name := range []string{"APP_ENV", "HTTP_ADDR", "DATABASE_URL", "LOGIN_CODE_SECRET", "RESEND_API_KEY", "MAIL_FROM"} {
		t.Setenv(name, values[name])
	}
}

func TestLocalDefaults(t *testing.T) {
	setEnv(t, nil)

	cfg, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Environment != "local" || cfg.DatabaseURL != LocalDatabaseURL || cfg.HTTPAddress != "127.0.0.1:8310" {
		t.Fatalf("cfg = %+v", cfg)
	}
}

func TestRequiredSettingsOutsideLocal(t *testing.T) {
	secret := strings.Repeat("s", 32)
	cases := []struct {
		name   string
		values map[string]string
		want   string
	}{
		{"unknown environment", map[string]string{"APP_ENV": "dev"}, "tidak dikenal"},
		{"staging without database", map[string]string{"APP_ENV": "staging", "LOGIN_CODE_SECRET": secret}, "DATABASE_URL"},
		{"staging without secret", map[string]string{"APP_ENV": "staging", "DATABASE_URL": "postgres://x"}, "LOGIN_CODE_SECRET wajib"},
		{"short secret", map[string]string{"LOGIN_CODE_SECRET": "pendek"}, "minimal 32"},
		{"production without resend", map[string]string{"APP_ENV": "production", "DATABASE_URL": "postgres://x", "LOGIN_CODE_SECRET": secret}, "RESEND_API_KEY"},
		{"resend without sender", map[string]string{"RESEND_API_KEY": "re_x"}, "MAIL_FROM"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			setEnv(t, tc.values)
			_, err := Load()
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err = %v, want mention of %q", err, tc.want)
			}
		})
	}
}

func TestStagingWithEverythingSet(t *testing.T) {
	setEnv(t, map[string]string{
		"APP_ENV":           "staging",
		"DATABASE_URL":      "postgres://lentera@db/lentera",
		"LOGIN_CODE_SECRET": strings.Repeat("s", 40),
	})

	cfg, err := Load()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.DatabaseURL != "postgres://lentera@db/lentera" {
		t.Fatalf("DatabaseURL = %q", cfg.DatabaseURL)
	}
}
