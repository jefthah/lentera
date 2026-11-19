// Package httpapi serves the Lentera sync API under /v1. Devices authenticate with a bearer token
// they receive after entering an email code; there are no cookies. See docs/SINKRON.md.
package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"net/netip"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/jefthah/lentera/server/internal/auth"
	"github.com/jefthah/lentera/server/internal/library"
	"github.com/jefthah/lentera/server/internal/mail"
	"github.com/jefthah/lentera/server/internal/ratelimit"
)

const (
	// maximumRequestBytes bounds ordinary requests; sync requests may be larger.
	maximumRequestBytes = 1 << 20
	// MaximumSyncBytes bounds one sync request. The app splits larger uploads.
	MaximumSyncBytes = 16 << 20
)

// developmentLoginCodeSecret keys stored codes when no secret is configured (tests and local).
const developmentLoginCodeSecret = "lentera-local-development-login-code-secret"

// Settings configures the parts of the API that depend on the environment.
type Settings struct {
	// LoginCodeSecret keys the HMAC of stored sign-in codes.
	LoginCodeSecret string
	// Mailer sends sign-in codes. Nil writes them to the log.
	Mailer mail.Mailer
}

// Server implements the API on top of PostgreSQL.
type Server struct {
	pool    *pgxpool.Pool
	logger  *slog.Logger
	auth    *auth.Service
	library *library.Store
	limiter *ratelimit.Limiter
}

// NewServer creates the API implementation.
func NewServer(pool *pgxpool.Pool, logger *slog.Logger, settings Settings) *Server {
	secret := settings.LoginCodeSecret
	if secret == "" {
		secret = developmentLoginCodeSecret
	}
	mailer := settings.Mailer
	if mailer == nil {
		mailer = mail.LogMailer{Logger: logger}
	}

	return &Server{
		pool:    pool,
		logger:  logger,
		auth:    auth.NewService(pool, mailer, []byte(secret)),
		library: library.NewStore(pool),
		limiter: ratelimit.New(),
	}
}

// Auth returns the sign-in service, for maintenance tasks.
func (s *Server) Auth() *auth.Service { return s.auth }

// Library returns the profile store, so tests can shrink its page size.
func (s *Server) Library() *library.Store { return s.library }

// NewHandler routes the API and adds the shared HTTP safeguards.
func NewHandler(s *Server) http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/health", s.health)
	mux.HandleFunc("POST /v1/auth/code", s.requestCode)
	mux.HandleFunc("POST /v1/auth/verify", s.verifyCode)
	mux.HandleFunc("POST /v1/auth/logout", s.signedIn(s.logout))
	mux.HandleFunc("GET /v1/me", s.signedIn(s.me))
	mux.HandleFunc("GET /v1/profiles", s.signedIn(s.listProfiles))
	mux.HandleFunc("PUT /v1/profiles/{id}", s.signedIn(s.putProfile))
	mux.HandleFunc("DELETE /v1/profiles/{id}", s.signedIn(s.deleteProfile))
	mux.HandleFunc("POST /v1/profiles/{id}/sync", s.signedIn(s.sync))
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		writeError(w, http.StatusNotFound, "not_found", "Alamat API tidak ditemukan.", "")
	})

	return withSafeguards(mux, s.logger)
}

func (s *Server) health(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Second)
	defer cancel()
	if err := s.pool.Ping(ctx); err != nil {
		s.logger.WarnContext(ctx, "database tidak dapat dihubungi", "error", err)
		writeJSON(w, http.StatusServiceUnavailable, map[string]string{"status": "degraded", "database": "unavailable"})
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"status": "ok", "database": "ok"})
}

func (s *Server) requestCode(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Email string `json:"email"`
	}
	if !decode(w, r, &body) {
		return
	}
	if !s.allow(w, "code-ip:"+clientIP(r), 6, time.Minute) {
		return
	}
	email, invalid := auth.NormalizeEmail(body.Email)
	if invalid != nil {
		s.authFailure(w, r, invalid)
		return
	}
	if !s.allow(w, "code-email:"+email, 8, time.Hour) {
		return
	}

	status, err := s.auth.RequestCode(r.Context(), email)
	if err != nil {
		s.authFailure(w, r, err)
		return
	}

	writeJSON(w, http.StatusOK, map[string]any{
		"email":                status.Email,
		"sent":                 status.Sent,
		"seconds_until_resend": status.SecondsUntilResend,
		"expires_in_seconds":   status.ExpiresInSeconds,
	})
}

func (s *Server) verifyCode(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Email  string `json:"email"`
		Code   string `json:"code"`
		Device string `json:"device"`
	}
	if !decode(w, r, &body) {
		return
	}
	if !s.allow(w, "verify-ip:"+clientIP(r), 10, time.Minute) {
		return
	}

	account, token, err := s.auth.VerifyCode(r.Context(), body.Email, body.Code, auth.Client{
		Device:    body.Device,
		IP:        clientIP(r),
		UserAgent: r.UserAgent(),
	})
	if err != nil {
		s.authFailure(w, r, err)
		return
	}

	writeJSON(w, http.StatusOK, map[string]any{"token": token, "account": accountBody(account)})
}

func (s *Server) logout(w http.ResponseWriter, r *http.Request, _ auth.Account, token string) {
	if err := s.auth.Logout(r.Context(), token); err != nil {
		s.serverError(w, r, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) me(w http.ResponseWriter, _ *http.Request, account auth.Account, _ string) {
	writeJSON(w, http.StatusOK, accountBody(account))
}

func (s *Server) listProfiles(w http.ResponseWriter, r *http.Request, account auth.Account, _ string) {
	profiles, err := s.library.Profiles(r.Context(), account.ID)
	if err != nil {
		s.serverError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"profiles": profiles})
}

func (s *Server) putProfile(w http.ResponseWriter, r *http.Request, account auth.Account, _ string) {
	var body library.ProfileInput
	if !decode(w, r, &body) {
		return
	}
	if !s.allow(w, fmt.Sprintf("profile:%d", account.ID), 60, time.Minute) {
		return
	}

	profile, created, err := s.library.PutProfile(r.Context(), account.ID, r.PathValue("id"), body)
	if err != nil {
		s.libraryFailure(w, r, err)
		return
	}
	status := http.StatusOK
	if created {
		status = http.StatusCreated
	}
	writeJSON(w, status, profile)
}

func (s *Server) deleteProfile(w http.ResponseWriter, r *http.Request, account auth.Account, _ string) {
	if !s.allow(w, fmt.Sprintf("profile:%d", account.ID), 60, time.Minute) {
		return
	}
	if err := s.library.DeleteProfile(r.Context(), account.ID, r.PathValue("id")); err != nil {
		s.libraryFailure(w, r, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) sync(w http.ResponseWriter, r *http.Request, account auth.Account, _ string) {
	var body library.SyncRequest
	if !decode(w, r, &body) {
		return
	}
	if !s.allow(w, fmt.Sprintf("sync:%d", account.ID), 240, time.Minute) {
		return
	}

	response, err := s.library.Sync(r.Context(), account.ID, r.PathValue("id"), body)
	if err != nil {
		s.libraryFailure(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, response)
}

// signedIn resolves the bearer token before calling next.
func (s *Server) signedIn(next func(http.ResponseWriter, *http.Request, auth.Account, string)) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		token, _ := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
		account, err := s.auth.Authenticate(r.Context(), strings.TrimSpace(token))
		if errors.Is(err, auth.ErrNoSession) {
			w.Header().Set("WWW-Authenticate", "Bearer")
			writeError(w, http.StatusUnauthorized, "unauthorized", "Sesi sudah berakhir. Masuk lagi dengan email Anda.", "")
			return
		}
		if err != nil {
			s.serverError(w, r, err)
			return
		}
		next(w, r, account, token)
	}
}

func (s *Server) allow(w http.ResponseWriter, key string, limit int, period time.Duration) bool {
	allowed, wait := s.limiter.Allow(key, limit, period)
	if allowed {
		return true
	}
	seconds := max(1, int((wait+time.Second-1)/time.Second))
	w.Header().Set("Retry-After", fmt.Sprint(seconds))
	writeError(w, http.StatusTooManyRequests, "rate_limited", fmt.Sprintf("Terlalu banyak permintaan. Coba lagi dalam %d detik.", seconds), "")

	return false
}

func (s *Server) authFailure(w http.ResponseWriter, r *http.Request, err error) {
	var failure *auth.Error
	if !errors.As(err, &failure) {
		s.serverError(w, r, err)
		return
	}
	status := http.StatusUnprocessableEntity
	switch failure.Kind {
	case auth.KindForbidden:
		status = http.StatusForbidden
	case auth.KindUnavailable:
		status = http.StatusServiceUnavailable
		s.logger.ErrorContext(r.Context(), "email kode masuk gagal", "error", failure.Unwrap())
	}
	writeError(w, status, failure.Code, failure.Message, failure.Field)
}

func (s *Server) libraryFailure(w http.ResponseWriter, r *http.Request, err error) {
	var failure *library.Error
	if !errors.As(err, &failure) {
		s.serverError(w, r, err)
		return
	}
	status := map[library.Kind]int{
		library.KindInvalid:   http.StatusUnprocessableEntity,
		library.KindNotFound:  http.StatusNotFound,
		library.KindForbidden: http.StatusForbidden,
		library.KindConflict:  http.StatusConflict,
		library.KindGone:      http.StatusGone,
	}[failure.Kind]
	writeError(w, status, failure.Code, failure.Message, failure.Field)
}

func (s *Server) serverError(w http.ResponseWriter, r *http.Request, err error) {
	s.logger.ErrorContext(r.Context(), "permintaan gagal", "method", r.Method, "path", r.URL.Path, "error", err)
	writeError(w, http.StatusInternalServerError, "server_error", "Terjadi kesalahan pada server. Coba lagi nanti.", "")
}

// decode reads a JSON body. Unknown fields are ignored so newer apps can talk to older servers.
func decode(w http.ResponseWriter, r *http.Request, target any) bool {
	err := json.NewDecoder(r.Body).Decode(target)
	if err == nil {
		return true
	}
	var tooLarge *http.MaxBytesError
	if errors.As(err, &tooLarge) {
		writeError(w, http.StatusRequestEntityTooLarge, "request_too_large", "Permintaan terlalu besar. Kirim dalam beberapa bagian.", "")
		return false
	}
	writeError(w, http.StatusBadRequest, "bad_request", "Permintaan tidak valid.", "")

	return false
}

func withSafeguards(next http.Handler, logger *slog.Logger) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		started := time.Now()
		recorder := &statusRecorder{ResponseWriter: w, status: http.StatusOK}

		recorder.Header().Set("X-Content-Type-Options", "nosniff")
		recorder.Header().Set("Cache-Control", "no-store")
		limit := int64(maximumRequestBytes)
		if strings.HasSuffix(r.URL.Path, "/sync") {
			limit = MaximumSyncBytes
		}
		r.Body = http.MaxBytesReader(recorder, r.Body, limit)

		next.ServeHTTP(recorder, r)

		logger.InfoContext(r.Context(), "permintaan", "method", r.Method, "path", r.URL.Path, "status", recorder.status, "duration_ms", time.Since(started).Milliseconds())
	})
}

type statusRecorder struct {
	http.ResponseWriter
	status int
}

func (r *statusRecorder) WriteHeader(status int) {
	r.status = status
	r.ResponseWriter.WriteHeader(status)
}

func accountBody(account auth.Account) map[string]any {
	return map[string]any{"id": account.ID, "email": account.Email}
}

func writeJSON(w http.ResponseWriter, status int, body any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(body)
}

func writeError(w http.ResponseWriter, status int, code, message, field string) {
	detail := map[string]string{"code": code, "message": message}
	if field != "" {
		detail["field"] = field
	}
	writeJSON(w, status, map[string]any{"error": detail})
}

// clientIP returns the device's address. Behind Caddy the TCP peer is a private or loopback
// address and the device is the last entry Caddy wrote to X-Forwarded-For.
func clientIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		host = r.RemoteAddr
	}
	peer, err := netip.ParseAddr(host)
	if err == nil && (peer.IsLoopback() || peer.IsPrivate()) {
		if forwarded := r.Header.Get("X-Forwarded-For"); forwarded != "" {
			entries := strings.Split(forwarded, ",")
			if candidate, err := netip.ParseAddr(strings.TrimSpace(entries[len(entries)-1])); err == nil {
				return candidate.Unmap().String()
			}
		}
	}
	if err != nil {
		return host
	}

	return peer.Unmap().String()
}
