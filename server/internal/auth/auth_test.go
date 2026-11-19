package auth_test

import (
	"context"
	"errors"
	"sync"
	"testing"

	"github.com/jefthah/lentera/server/internal/auth"
	"github.com/jefthah/lentera/server/internal/mail"
	"github.com/jefthah/lentera/server/internal/testdb"
)

type captureMailer struct {
	mu    sync.Mutex
	codes map[string]string
	sent  int
	fail  error
}

func (m *captureMailer) SendLoginCode(_ context.Context, message mail.LoginCode) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.fail != nil {
		return m.fail
	}
	if m.codes == nil {
		m.codes = map[string]string{}
	}
	m.codes[message.To] = message.Code
	m.sent++
	return nil
}

func wrongCode(code string) string {
	if code == "000000" {
		return "111111"
	}
	return "000000"
}

func authError(t *testing.T, err error, code string) {
	t.Helper()
	var failure *auth.Error
	if !errors.As(err, &failure) || failure.Code != code {
		t.Fatalf("err = %v, want auth error %q", err, code)
	}
}

func TestNormalizeEmail(t *testing.T) {
	email, invalid := auth.NormalizeEmail("  Budi@Gereja.ORG ")
	if invalid != nil || email != "budi@gereja.org" {
		t.Fatalf("got %q, %v", email, invalid)
	}
	for _, raw := range []string{"", "budi", "budi@gereja", "bu di@gereja.org", ".budi@gereja.org", "budi@-gereja.org"} {
		if _, invalid := auth.NormalizeEmail(raw); invalid == nil {
			t.Errorf("%q accepted", raw)
		}
	}
}

func TestCodeSignInFlow(t *testing.T) {
	ctx := context.Background()
	mailer := &captureMailer{}
	service := auth.NewService(testdb.New(t), mailer, []byte("rahasia-tes"))

	status, err := service.RequestCode(ctx, "Budi@Gereja.org")
	if err != nil || !status.Sent || status.Email != "budi@gereja.org" {
		t.Fatalf("RequestCode = %+v, %v", status, err)
	}
	// Within the cooldown no second email goes out.
	again, err := service.RequestCode(ctx, "budi@gereja.org")
	if err != nil || again.Sent || again.SecondsUntilResend <= 0 || mailer.sent != 1 {
		t.Fatalf("second RequestCode = %+v, %v, sent %d", again, err, mailer.sent)
	}

	code := mailer.codes["budi@gereja.org"]
	_, _, err = service.VerifyCode(ctx, "budi@gereja.org", wrongCode(code), auth.Client{})
	authError(t, err, "invalid_code")

	account, token, err := service.VerifyCode(ctx, "budi@gereja.org", code[:3]+" "+code[3:], auth.Client{Device: "  Laptop   Mimbar ", IP: "203.0.113.7", UserAgent: "Lentera/0.3.0"})
	if err != nil || token == "" || account.Email != "budi@gereja.org" {
		t.Fatalf("VerifyCode = %+v, %q, %v", account, token, err)
	}

	// The code works only once.
	_, _, err = service.VerifyCode(ctx, "budi@gereja.org", code, auth.Client{})
	authError(t, err, "code_expired")

	signedIn, err := service.Authenticate(ctx, token)
	if err != nil || signedIn != account {
		t.Fatalf("Authenticate = %+v, %v", signedIn, err)
	}

	if err := service.Logout(ctx, token); err != nil {
		t.Fatal(err)
	}
	if err := service.Logout(ctx, token); err != nil {
		t.Fatalf("second logout: %v", err)
	}
	if _, err := service.Authenticate(ctx, token); !errors.Is(err, auth.ErrNoSession) {
		t.Fatalf("after logout err = %v", err)
	}
}

func TestTooManyWrongCodesDiscardTheCode(t *testing.T) {
	ctx := context.Background()
	mailer := &captureMailer{}
	service := auth.NewService(testdb.New(t), mailer, []byte("rahasia-tes"))

	if _, err := service.RequestCode(ctx, "ani@gereja.org"); err != nil {
		t.Fatal(err)
	}
	code := mailer.codes["ani@gereja.org"]
	for range auth.MaxAttempts - 1 {
		_, _, err := service.VerifyCode(ctx, "ani@gereja.org", wrongCode(code), auth.Client{})
		authError(t, err, "invalid_code")
	}
	_, _, err := service.VerifyCode(ctx, "ani@gereja.org", wrongCode(code), auth.Client{})
	authError(t, err, "too_many_attempts")
	_, _, err = service.VerifyCode(ctx, "ani@gereja.org", code, auth.Client{})
	authError(t, err, "code_expired")
}

func TestSessionsEndWhenExpiredOrDisabled(t *testing.T) {
	ctx := context.Background()
	pool := testdb.New(t)
	mailer := &captureMailer{}
	service := auth.NewService(pool, mailer, []byte("rahasia-tes"))

	signIn := func(email string) string {
		t.Helper()
		if _, err := service.RequestCode(ctx, email); err != nil {
			t.Fatal(err)
		}
		_, token, err := service.VerifyCode(ctx, email, mailer.codes[email], auth.Client{})
		if err != nil {
			t.Fatal(err)
		}
		return token
	}

	expired := signIn("lama@gereja.org")
	if _, err := pool.Exec(ctx, `UPDATE sessions SET expires_at = now() - interval '1 second'`); err != nil {
		t.Fatal(err)
	}
	if _, err := service.Authenticate(ctx, expired); !errors.Is(err, auth.ErrNoSession) {
		t.Fatalf("expired session err = %v", err)
	}
	if err := service.RemoveExpired(ctx); err != nil {
		t.Fatal(err)
	}

	// An old session is extended when used.
	active := signIn("aktif@gereja.org")
	if _, err := pool.Exec(ctx, `UPDATE sessions SET last_seen_at = now() - interval '2 days', expires_at = now() + interval '1 hour'`); err != nil {
		t.Fatal(err)
	}
	if _, err := service.Authenticate(ctx, active); err != nil {
		t.Fatal(err)
	}
	var days int
	if err := pool.QueryRow(ctx, `SELECT extract(day FROM expires_at - now())::int FROM sessions`).Scan(&days); err != nil || days < 179 {
		t.Fatalf("session not extended: %d days, %v", days, err)
	}

	if _, err := pool.Exec(ctx, `UPDATE accounts SET disabled_at = now()`); err != nil {
		t.Fatal(err)
	}
	if _, err := service.Authenticate(ctx, active); !errors.Is(err, auth.ErrNoSession) {
		t.Fatalf("disabled account err = %v", err)
	}
	if _, err := pool.Exec(ctx, `DELETE FROM login_codes`); err != nil {
		t.Fatal(err)
	}
	if _, err := service.RequestCode(ctx, "aktif@gereja.org"); err != nil {
		t.Fatal(err)
	}
	_, _, err := service.VerifyCode(ctx, "aktif@gereja.org", mailer.codes["aktif@gereja.org"], auth.Client{})
	authError(t, err, "account_disabled")
}

func TestFailedEmailLetsThePersonRetryAtOnce(t *testing.T) {
	ctx := context.Background()
	mailer := &captureMailer{fail: errors.New("resend mati")}
	service := auth.NewService(testdb.New(t), mailer, []byte("rahasia-tes"))

	_, err := service.RequestCode(ctx, "budi@gereja.org")
	authError(t, err, "mail_failed")

	mailer.fail = nil
	status, err := service.RequestCode(ctx, "budi@gereja.org")
	if err != nil || !status.Sent {
		t.Fatalf("retry = %+v, %v", status, err)
	}
}
