package httpapi_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"

	"github.com/jefthah/lentera/server/internal/httpapi"
	"github.com/jefthah/lentera/server/internal/mail"
	"github.com/jefthah/lentera/server/internal/testdb"
)

type captureMailer struct {
	mu    sync.Mutex
	codes map[string]string
}

func (m *captureMailer) SendLoginCode(_ context.Context, message mail.LoginCode) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.codes == nil {
		m.codes = map[string]string{}
	}
	m.codes[message.To] = message.Code
	return nil
}

type client struct {
	t       *testing.T
	handler http.Handler
	token   string
}

func (c *client) do(method, path string, body any) (int, map[string]any) {
	c.t.Helper()
	var reader io.Reader
	if body != nil {
		encoded, err := json.Marshal(body)
		if err != nil {
			c.t.Fatal(err)
		}
		reader = bytes.NewReader(encoded)
	}
	request := httptest.NewRequest(method, path, reader)
	request.RemoteAddr = "203.0.113.9:5000"
	if c.token != "" {
		request.Header.Set("Authorization", "Bearer "+c.token)
	}
	recorder := httptest.NewRecorder()
	c.handler.ServeHTTP(recorder, request)

	var decoded map[string]any
	if recorder.Body.Len() > 0 {
		if err := json.Unmarshal(recorder.Body.Bytes(), &decoded); err != nil {
			c.t.Fatalf("%s %s: body is not JSON: %q", method, path, recorder.Body.String())
		}
	}
	return recorder.Code, decoded
}

func errorCode(body map[string]any) string {
	detail, _ := body["error"].(map[string]any)
	code, _ := detail["code"].(string)
	return code
}

func quietLogger() *slog.Logger { return slog.New(slog.NewTextHandler(io.Discard, nil)) }

const profile = "0190d2a0-0000-7000-8000-000000000100"

func TestSignInAndSyncOverHTTP(t *testing.T) {
	mailer := &captureMailer{}
	api := httpapi.NewServer(testdb.New(t), quietLogger(), httpapi.Settings{Mailer: mailer})
	c := &client{t: t, handler: httpapi.NewHandler(api)}

	if status, body := c.do("GET", "/v1/health", nil); status != 200 || body["database"] != "ok" {
		t.Fatalf("health = %d %v", status, body)
	}
	if status, body := c.do("GET", "/v1/profiles", nil); status != 401 || errorCode(body) != "unauthorized" {
		t.Fatalf("profiles without token = %d %v", status, body)
	}

	status, body := c.do("POST", "/v1/auth/code", map[string]string{"email": "Budi@Gereja.org"})
	if status != 200 || body["sent"] != true {
		t.Fatalf("code = %d %v", status, body)
	}
	status, body = c.do("POST", "/v1/auth/verify", map[string]string{"email": "budi@gereja.org", "code": "12", "device": "Laptop"})
	if status != 422 || errorCode(body) != "validation_failed" {
		t.Fatalf("short code = %d %v", status, body)
	}
	status, body = c.do("POST", "/v1/auth/verify", map[string]string{"email": "budi@gereja.org", "code": mailer.codes["budi@gereja.org"], "device": "Laptop"})
	if status != 200 || body["token"] == "" {
		t.Fatalf("verify = %d %v", status, body)
	}
	c.token = body["token"].(string)

	if status, body := c.do("GET", "/v1/me", nil); status != 200 || body["email"] != "budi@gereja.org" {
		t.Fatalf("me = %d %v", status, body)
	}

	status, body = c.do("PUT", "/v1/profiles/"+profile, map[string]any{"name": "Ibadah Umum", "created_at": 1_800_000_000_000, "updated_at": 1_800_000_000_000})
	if status != 201 || body["role"] != "owner" {
		t.Fatalf("create profile = %d %v", status, body)
	}
	status, _ = c.do("PUT", "/v1/profiles/"+profile, map[string]any{"name": "Ibadah Umum", "created_at": 1_800_000_000_000, "updated_at": 1_800_000_000_000})
	if status != 200 {
		t.Fatalf("repeat create = %d", status)
	}

	status, body = c.do("POST", "/v1/profiles/"+profile+"/sync", map[string]any{
		"cursor": 0,
		"items": []map[string]any{{
			"id": "0190d2a0-0000-7000-8000-000000000001", "kind": "song", "title": "Kasih-Mu ✓", "content": "Bait satu\n\nBait dua",
			"created_at": 1_700_000_000_000, "updated_at": 1_700_000_000_000, "deleted_at": nil,
		}},
		"service":  map[string]any{"items": []string{"0190d2a0-0000-7000-8000-000000000001"}, "updated_at": 1_700_000_000_000},
		"settings": nil,
	})
	if status != 200 || body["accepted"] != float64(2) || body["cursor"] != float64(2) {
		t.Fatalf("sync = %d %v", status, body)
	}

	status, body = c.do("GET", "/v1/profiles", nil)
	list, _ := body["profiles"].([]any)
	if status != 200 || len(list) != 1 || list[0].(map[string]any)["revision"] != float64(2) {
		t.Fatalf("profiles = %d %v", status, body)
	}

	if status, body := c.do("POST", "/v1/profiles/"+profile+"/sync", map[string]any{"service": map[string]any{"items": []string{"0190d2a0-0000-7000-8000-000000000009"}, "updated_at": 1_700_000_000_001}}); status != 422 || errorCode(body) != "service_unknown_item" {
		t.Fatalf("unknown service item = %d %v", status, body)
	}
	if status, body := c.do("POST", "/v1/profiles/0190d2a0-0000-7000-8000-000000000999/sync", map[string]any{}); status != 404 || errorCode(body) != "profile_not_found" {
		t.Fatalf("unknown profile = %d %v", status, body)
	}

	if status, _ := c.do("DELETE", "/v1/profiles/"+profile, nil); status != 204 {
		t.Fatalf("delete = %d", status)
	}
	if status, body := c.do("POST", "/v1/profiles/"+profile+"/sync", map[string]any{}); status != 410 || errorCode(body) != "profile_deleted" {
		t.Fatalf("sync deleted = %d %v", status, body)
	}

	if status, _ := c.do("POST", "/v1/auth/logout", nil); status != 204 {
		t.Fatalf("logout = %d", status)
	}
	if status, _ := c.do("GET", "/v1/me", nil); status != 401 {
		t.Fatalf("me after logout = %d", status)
	}
}

func TestMalformedRequestsGetJSONErrors(t *testing.T) {
	api := httpapi.NewServer(testdb.New(t), quietLogger(), httpapi.Settings{Mailer: &captureMailer{}})
	handler := httpapi.NewHandler(api)
	c := &client{t: t, handler: handler}

	if status, body := c.do("GET", "/v2/apa", nil); status != 404 || errorCode(body) != "not_found" {
		t.Fatalf("unknown path = %d %v", status, body)
	}

	request := httptest.NewRequest("POST", "/v1/auth/code", strings.NewReader("{bukan json"))
	recorder := httptest.NewRecorder()
	handler.ServeHTTP(recorder, request)
	if recorder.Code != 400 || !strings.Contains(recorder.Body.String(), "bad_request") {
		t.Fatalf("bad JSON = %d %s", recorder.Code, recorder.Body.String())
	}

	big := `{"email":"` + strings.Repeat("a", 2<<20) + `"}`
	request = httptest.NewRequest("POST", "/v1/auth/code", strings.NewReader(big))
	recorder = httptest.NewRecorder()
	handler.ServeHTTP(recorder, request)
	if recorder.Code != 413 {
		t.Fatalf("large body = %d", recorder.Code)
	}

	for i := range 7 {
		status, body := c.do("POST", "/v1/auth/code", map[string]string{"email": "bukan-email"})
		if i < 6 && status != 422 {
			t.Fatalf("attempt %d = %d %v", i, status, body)
		}
		if i == 6 && (status != 429 || errorCode(body) != "rate_limited") {
			t.Fatalf("attempt %d = %d %v", i, status, body)
		}
	}
}
