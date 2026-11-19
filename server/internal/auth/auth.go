// Package auth signs people in to Lentera with six-digit email codes and keeps one long-lived
// session per device. There are no passwords.
package auth

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"fmt"
	"math/big"
	"net/netip"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/jefthah/lentera/server/internal/mail"
)

const (
	// CodeLifetime is how long a sign-in code stays valid.
	CodeLifetime = 10 * time.Minute
	// ResendCooldown is the minimum time between two code emails to the same address.
	ResendCooldown = 60 * time.Second
	// MaxAttempts is the number of wrong codes after which the code is discarded.
	MaxAttempts = 5
	// SessionLifetime is how long a device stays signed in without contacting the server. Lentera
	// must keep working offline during a service, so this is long and extended on every use.
	SessionLifetime = 180 * 24 * time.Hour
	// touchInterval limits how often a session's expiry is pushed back.
	touchInterval = 24 * time.Hour
	// DeviceNameMax is the longest device name kept on a session.
	DeviceNameMax = 100
)

// ErrNoSession means the request carries no valid session.
var ErrNoSession = errors.New("sesi tidak ditemukan atau sudah berakhir")

// Kind classifies an Error so the HTTP layer can choose a status code.
type Kind int

const (
	// KindInvalid is a problem with the submitted input (HTTP 422).
	KindInvalid Kind = iota + 1
	// KindForbidden means the account may not sign in (HTTP 403).
	KindForbidden
	// KindUnavailable means a dependency such as email delivery failed (HTTP 503).
	KindUnavailable
)

// Error is a sign-in failure the person can act on. Message is shown to them as-is.
type Error struct {
	Kind    Kind
	Code    string
	Message string
	Field   string
	cause   error
}

func (e *Error) Error() string { return e.Message }

func (e *Error) Unwrap() error { return e.cause }

// Account is the signed-in person as exposed to the rest of the server.
type Account struct {
	ID    int64
	Email string
}

// Client describes where a sign-in comes from; it is stored on the session.
type Client struct {
	Device    string
	IP        string
	UserAgent string
}

// CodeStatus reports the state of the active code after a send request.
type CodeStatus struct {
	Email              string
	Sent               bool
	SecondsUntilResend int
	ExpiresInSeconds   int
}

// Service signs people in. It is safe for concurrent use.
type Service struct {
	pool   *pgxpool.Pool
	mailer mail.Mailer
	secret []byte
}

// NewService creates the sign-in service. secret keys the HMAC that protects stored codes.
func NewService(pool *pgxpool.Pool, mailer mail.Mailer, secret []byte) *Service {
	return &Service{pool: pool, mailer: mailer, secret: secret}
}

type storedCode struct {
	hash               []byte
	expired            bool
	secondsUntilResend int
	expiresInSeconds   int
}

// RequestCode emails a new code unless one was sent during the resend cooldown.
func (s *Service) RequestCode(ctx context.Context, rawEmail string) (CodeStatus, error) {
	email, invalid := NormalizeEmail(rawEmail)
	if invalid != nil {
		return CodeStatus{}, invalid
	}

	code, err := generateCode()
	if err != nil {
		return CodeStatus{}, err
	}

	cooling := false
	status := CodeStatus{Email: email}
	err = pgx.BeginFunc(ctx, s.pool, func(tx pgx.Tx) error {
		current, err := lockCode(ctx, tx, email)
		if err == nil && !current.expired && current.secondsUntilResend > 0 {
			cooling = true
			status.SecondsUntilResend = current.secondsUntilResend
			status.ExpiresInSeconds = current.expiresInSeconds
			return nil
		}
		if err != nil && !errors.Is(err, pgx.ErrNoRows) {
			return err
		}

		_, err = tx.Exec(ctx, `
			INSERT INTO login_codes (email, code_hash, attempts, sent_at, expires_at)
			VALUES ($1, $2, 0, now(), now() + make_interval(secs => $3))
			ON CONFLICT (email) DO UPDATE
			SET code_hash = excluded.code_hash, attempts = 0, sent_at = excluded.sent_at, expires_at = excluded.expires_at`,
			email, s.hashCode(email, code), CodeLifetime.Seconds())
		return err
	})
	if err != nil {
		return CodeStatus{}, fmt.Errorf("menyimpan kode masuk: %w", err)
	}
	if cooling {
		return status, nil
	}

	if err := s.mailer.SendLoginCode(ctx, mail.LoginCode{To: email, Code: code, ExpiresIn: CodeLifetime}); err != nil {
		// Without the email the code is useless; remove it so the person can retry at once.
		_, _ = s.pool.Exec(context.WithoutCancel(ctx), `DELETE FROM login_codes WHERE email = $1`, email)
		return CodeStatus{}, &Error{
			Kind:    KindUnavailable,
			Code:    "mail_failed",
			Message: "Kode belum bisa dikirim. Coba lagi dalam beberapa saat.",
			Field:   "email",
			cause:   err,
		}
	}

	status.Sent = true
	status.SecondsUntilResend = int(ResendCooldown.Seconds())
	status.ExpiresInSeconds = int(CodeLifetime.Seconds())

	return status, nil
}

// VerifyCode checks a code and, when it matches, signs the device in. An unknown email gets a new
// account. It returns the account and the session token the app keeps.
func (s *Service) VerifyCode(ctx context.Context, rawEmail, rawCode string, client Client) (Account, string, error) {
	email, invalid := NormalizeEmail(rawEmail)
	if invalid != nil {
		return Account{}, "", invalid
	}
	code := digitsOnly(rawCode)
	if len(code) != 6 {
		return Account{}, "", &Error{Kind: KindInvalid, Code: "validation_failed", Field: "code", Message: "Masukkan 6 digit kode dari email."}
	}

	var (
		account  Account
		token    string
		rejected *Error
	)
	err := pgx.BeginFunc(ctx, s.pool, func(tx pgx.Tx) error {
		current, err := lockCode(ctx, tx, email)
		if errors.Is(err, pgx.ErrNoRows) || (err == nil && current.expired) {
			rejected = &Error{Kind: KindInvalid, Code: "code_expired", Field: "code", Message: "Kode sudah kedaluwarsa. Kirim kode baru untuk melanjutkan."}
			_, err = tx.Exec(ctx, `DELETE FROM login_codes WHERE email = $1`, email)
			return err
		}
		if err != nil {
			return err
		}

		if !hmac.Equal(s.hashCode(email, code), current.hash) {
			var attempts int
			err := tx.QueryRow(ctx, `UPDATE login_codes SET attempts = attempts + 1 WHERE email = $1 RETURNING attempts`, email).Scan(&attempts)
			if err != nil {
				return err
			}
			if attempts >= MaxAttempts {
				rejected = &Error{Kind: KindInvalid, Code: "too_many_attempts", Field: "code", Message: "Kode salah terlalu sering. Kirim kode baru untuk melanjutkan."}
				_, err = tx.Exec(ctx, `DELETE FROM login_codes WHERE email = $1`, email)
				return err
			}
			rejected = &Error{Kind: KindInvalid, Code: "invalid_code", Field: "code", Message: fmt.Sprintf("Kode tidak cocok. Sisa %d percobaan.", MaxAttempts-attempts)}
			return nil
		}

		if _, err := tx.Exec(ctx, `DELETE FROM login_codes WHERE email = $1`, email); err != nil {
			return err
		}

		var disabled bool
		err = tx.QueryRow(ctx, `
			INSERT INTO accounts (email) VALUES ($1)
			ON CONFLICT (email) DO UPDATE SET email = excluded.email
			RETURNING id, email, disabled_at IS NOT NULL`, email).Scan(&account.ID, &account.Email, &disabled)
		if err != nil {
			return err
		}
		if disabled {
			rejected = &Error{Kind: KindForbidden, Code: "account_disabled", Message: "Akun ini sedang dinonaktifkan."}
			_, err = tx.Exec(ctx, `DELETE FROM sessions WHERE account_id = $1`, account.ID)
			return err
		}

		token, err = createSession(ctx, tx, account.ID, client)
		return err
	})
	if err != nil {
		return Account{}, "", fmt.Errorf("memeriksa kode masuk: %w", err)
	}
	if rejected != nil {
		return Account{}, "", rejected
	}

	return account, token, nil
}

// Authenticate returns the account behind a session token and extends the session. It returns
// ErrNoSession for unknown, expired, or disabled sessions.
func (s *Service) Authenticate(ctx context.Context, token string) (Account, error) {
	if token == "" {
		return Account{}, ErrNoSession
	}
	tokenHash := hashToken(token)

	var (
		account  Account
		disabled bool
		stale    bool
	)
	err := s.pool.QueryRow(ctx, `
		SELECT a.id, a.email, a.disabled_at IS NOT NULL, s.last_seen_at < now() - make_interval(secs => $2)
		FROM sessions s JOIN accounts a ON a.id = s.account_id
		WHERE s.token_hash = $1 AND s.expires_at > now()`,
		tokenHash, touchInterval.Seconds()).Scan(&account.ID, &account.Email, &disabled, &stale)
	if errors.Is(err, pgx.ErrNoRows) {
		return Account{}, ErrNoSession
	}
	if err != nil {
		return Account{}, fmt.Errorf("membaca sesi: %w", err)
	}
	if disabled {
		if _, err := s.pool.Exec(ctx, `DELETE FROM sessions WHERE account_id = $1`, account.ID); err != nil {
			return Account{}, fmt.Errorf("menghapus sesi akun nonaktif: %w", err)
		}
		return Account{}, ErrNoSession
	}
	if stale {
		_, err := s.pool.Exec(ctx, `
			UPDATE sessions SET last_seen_at = now(), expires_at = now() + make_interval(secs => $2)
			WHERE token_hash = $1`, tokenHash, SessionLifetime.Seconds())
		if err != nil {
			return Account{}, fmt.Errorf("memperpanjang sesi: %w", err)
		}
	}

	return account, nil
}

// Logout deletes the session behind token. Unknown tokens are ignored.
func (s *Service) Logout(ctx context.Context, token string) error {
	if token == "" {
		return nil
	}
	_, err := s.pool.Exec(ctx, `DELETE FROM sessions WHERE token_hash = $1`, hashToken(token))

	return err
}

// RemoveExpired deletes sessions and codes that can no longer be used.
func (s *Service) RemoveExpired(ctx context.Context) error {
	if _, err := s.pool.Exec(ctx, `DELETE FROM sessions WHERE expires_at <= now()`); err != nil {
		return err
	}
	_, err := s.pool.Exec(ctx, `DELETE FROM login_codes WHERE expires_at <= now()`)

	return err
}

// NormalizeEmail trims and lower-cases an email address and checks that it is usable.
func NormalizeEmail(raw string) (string, *Error) {
	email := strings.ToLower(strings.TrimSpace(raw))
	invalid := func(message string) *Error {
		return &Error{Kind: KindInvalid, Code: "validation_failed", Field: "email", Message: message}
	}

	switch {
	case email == "":
		return "", invalid("Masukkan alamat email Anda.")
	case len(email) > 255:
		return "", invalid("Alamat email terlalu panjang.")
	case !looksLikeEmail(email):
		return "", invalid("Format email belum benar. Contoh: nama@email.com")
	}

	return email, nil
}

// lockCode serialises work on one email address and reads its active code, if any.
func lockCode(ctx context.Context, tx pgx.Tx, email string) (storedCode, error) {
	if _, err := tx.Exec(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1, 0))`, "login:"+email); err != nil {
		return storedCode{}, err
	}
	var code storedCode
	err := tx.QueryRow(ctx, `
		SELECT code_hash,
		       expires_at <= now(),
		       greatest(0, ceil(extract(epoch FROM sent_at + make_interval(secs => $2) - now())))::int,
		       greatest(0, ceil(extract(epoch FROM expires_at - now())))::int
		FROM login_codes WHERE email = $1 FOR UPDATE`,
		email, ResendCooldown.Seconds()).Scan(&code.hash, &code.expired, &code.secondsUntilResend, &code.expiresInSeconds)

	return code, err
}

func (s *Service) hashCode(email, code string) []byte {
	mac := hmac.New(sha256.New, s.secret)
	mac.Write([]byte(email))
	mac.Write([]byte{0})
	mac.Write([]byte(code))

	return mac.Sum(nil)
}

func createSession(ctx context.Context, tx pgx.Tx, accountID int64, client Client) (string, error) {
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	token := base64.RawURLEncoding.EncodeToString(raw)

	var address *netip.Addr
	if parsed, err := netip.ParseAddr(client.IP); err == nil {
		address = &parsed
	}
	var agent *string
	if value := strings.ToValidUTF8(client.UserAgent, ""); value != "" {
		value = truncate(value, 500)
		agent = &value
	}
	device := truncate(strings.Join(strings.Fields(strings.ToValidUTF8(client.Device, "")), " "), DeviceNameMax)

	_, err := tx.Exec(ctx, `
		INSERT INTO sessions (token_hash, account_id, device, user_agent, ip_address, expires_at)
		VALUES ($1, $2, $3, $4, $5, now() + make_interval(secs => $6))`,
		hashToken(token), accountID, device, agent, address, SessionLifetime.Seconds())

	return token, err
}

// truncate cuts s to at most limit characters without splitting one.
func truncate(s string, limit int) string {
	if utf8.RuneCountInString(s) <= limit {
		return s
	}
	runes := []rune(s)

	return string(runes[:limit])
}

func hashToken(token string) []byte {
	sum := sha256.Sum256([]byte(token))
	return sum[:]
}

func generateCode() (string, error) {
	number, err := rand.Int(rand.Reader, big.NewInt(1_000_000))
	if err != nil {
		return "", err
	}

	return fmt.Sprintf("%06d", number.Int64()), nil
}

func digitsOnly(value string) string {
	return strings.Map(func(r rune) rune {
		if r >= '0' && r <= '9' {
			return r
		}
		return -1
	}, value)
}

func looksLikeEmail(email string) bool {
	local, domain, found := strings.Cut(email, "@")
	if !found || local == "" || domain == "" || strings.Contains(domain, "@") {
		return false
	}
	if strings.ContainsAny(email, " \t\r\n<>()[]\\,;:\"") {
		return false
	}
	if strings.HasPrefix(local, ".") || strings.HasSuffix(local, ".") || strings.Contains(email, "..") {
		return false
	}
	labels := strings.Split(domain, ".")
	if len(labels) < 2 {
		return false
	}
	for _, label := range labels {
		if label == "" || strings.HasPrefix(label, "-") || strings.HasSuffix(label, "-") {
			return false
		}
	}

	return true
}
