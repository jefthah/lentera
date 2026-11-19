// Package mail sends Lentera's sign-in code emails through Resend, or writes them to the log when
// no Resend API key is configured.
package mail

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"html"
	"io"
	"log/slog"
	"net/http"
	"time"
)

// LoginCode is the content of a sign-in code email.
type LoginCode struct {
	To        string
	Code      string
	ExpiresIn time.Duration
}

// Mailer delivers sign-in code emails.
type Mailer interface {
	SendLoginCode(ctx context.Context, message LoginCode) error
}

// LogMailer writes the email to the log instead of sending it. It is meant for local development
// and for staging until a Resend API key is configured.
type LogMailer struct {
	Logger *slog.Logger
}

// SendLoginCode logs the code so whoever can read the server log can finish signing in.
func (m LogMailer) SendLoginCode(ctx context.Context, message LoginCode) error {
	m.Logger.InfoContext(ctx, "email kode masuk (mode log, tidak dikirim)",
		"to", message.To,
		"code", message.Code,
		"expires_in_minutes", int(message.ExpiresIn.Minutes()),
	)

	return nil
}

// DefaultResendEndpoint is Resend's send-email API.
const DefaultResendEndpoint = "https://api.resend.com/emails"

// ResendMailer sends emails through the Resend HTTP API.
type ResendMailer struct {
	APIKey   string
	From     string
	Endpoint string
	Client   *http.Client
}

// NewResendMailer creates a mailer with a bounded HTTP timeout.
func NewResendMailer(apiKey, from string) ResendMailer {
	return ResendMailer{
		APIKey:   apiKey,
		From:     from,
		Endpoint: DefaultResendEndpoint,
		Client:   &http.Client{Timeout: 10 * time.Second},
	}
}

// SendLoginCode sends the sign-in code email and returns an error when Resend does not accept it.
func (m ResendMailer) SendLoginCode(ctx context.Context, message LoginCode) error {
	subject, text, htmlBody := renderLoginCode(message)

	payload, err := json.Marshal(map[string]any{
		"from":    m.From,
		"to":      []string{message.To},
		"subject": subject,
		"text":    text,
		"html":    htmlBody,
	})
	if err != nil {
		return err
	}

	request, err := http.NewRequestWithContext(ctx, http.MethodPost, m.Endpoint, bytes.NewReader(payload))
	if err != nil {
		return err
	}
	request.Header.Set("Authorization", "Bearer "+m.APIKey)
	request.Header.Set("Content-Type", "application/json")

	response, err := m.Client.Do(request)
	if err != nil {
		return fmt.Errorf("mengirim email lewat Resend: %w", err)
	}
	defer response.Body.Close()

	if response.StatusCode < 200 || response.StatusCode > 299 {
		detail, _ := io.ReadAll(io.LimitReader(response.Body, 512))
		return fmt.Errorf("resend menolak email (status %d): %s", response.StatusCode, detail)
	}

	return nil
}

// renderLoginCode builds the subject and the plain-text and HTML bodies of the sign-in code email.
func renderLoginCode(message LoginCode) (subject, text, htmlBody string) {
	grouped := message.Code
	if len(grouped) == 6 {
		grouped = message.Code[:3] + " " + message.Code[3:]
	}
	minutes := int(message.ExpiresIn.Minutes())

	subject = message.Code + " adalah kode masuk Lentera Anda"
	text = fmt.Sprintf(
		"Kode masuk Lentera Anda: %s\n\nKetik kode ini di aplikasi Lentera. Kode berlaku %d menit "+
			"dan hanya bisa dipakai sekali. Jangan bagikan kode ini kepada siapa pun.\n\n"+
			"Jika Anda tidak sedang masuk ke Lentera, abaikan email ini.\n",
		grouped, minutes,
	)
	htmlBody = fmt.Sprintf(`<!doctype html>
<html lang="id"><body style="margin:0;background:#101217;font-family:Arial,Helvetica,sans-serif;color:#f2f3f5">
<table role="presentation" width="100%%" cellpadding="0" cellspacing="0" style="padding:32px 16px"><tr><td align="center">
<table role="presentation" width="100%%" cellpadding="0" cellspacing="0" style="max-width:480px;background:#181b22;border-radius:20px;padding:32px">
<tr><td style="font-size:15px;font-weight:bold;letter-spacing:1px;color:#f2b441">LENTERA</td></tr>
<tr><td style="padding-top:24px;font-size:22px;font-weight:bold">Kode masuk Lentera Anda</td></tr>
<tr><td style="padding-top:16px;font-size:36px;font-weight:bold;letter-spacing:6px">%s</td></tr>
<tr><td style="padding-top:16px;font-size:15px;line-height:22px;color:#a9aebb">Ketik kode ini di aplikasi Lentera. Kode berlaku %d menit dan hanya bisa dipakai sekali. Jangan bagikan kode ini kepada siapa pun.</td></tr>
<tr><td style="padding-top:16px;font-size:13px;line-height:20px;color:#a9aebb">Jika Anda tidak sedang masuk ke Lentera, abaikan email ini.</td></tr>
</table></td></tr></table></body></html>`, html.EscapeString(grouped), minutes)

	return subject, text, htmlBody
}
