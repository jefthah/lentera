package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestHealthURL(t *testing.T) {
	cases := map[string]string{
		"127.0.0.1:8310": "http://127.0.0.1:8310/v1/health",
		"0.0.0.0:8310":   "http://127.0.0.1:8310/v1/health",
		":8310":          "http://127.0.0.1:8310/v1/health",
		"[::]:8310":      "http://[::1]:8310/v1/health",
	}
	for addr, want := range cases {
		got, err := healthURL(addr)
		if err != nil || got != want {
			t.Errorf("healthURL(%q) = %q, %v; want %q", addr, got, err, want)
		}
	}
	if _, err := healthURL("8310"); err == nil {
		t.Error("healthURL without a port separator should fail")
	}
}

func TestCheckHealth(t *testing.T) {
	status := http.StatusOK
	api := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/health" {
			http.NotFound(w, r)
			return
		}
		w.WriteHeader(status)
	}))
	defer api.Close()
	addr := strings.TrimPrefix(api.URL, "http://")

	if err := checkHealth(addr); err != nil {
		t.Fatalf("healthy API: %v", err)
	}

	status = http.StatusServiceUnavailable
	if err := checkHealth(addr); err == nil || !strings.Contains(err.Error(), "503") {
		t.Fatalf("degraded API: err = %v, want 503", err)
	}

	api.Close()
	if err := checkHealth(addr); err == nil || !strings.Contains(err.Error(), "tidak menjawab") {
		t.Fatalf("stopped API: err = %v", err)
	}
}
