// Command lentera-server runs the Lentera sign-in and sync API and its maintenance tasks.
//
//	lentera-server                   menjalankan API (sama dengan "lentera-server serve")
//	lentera-server migrate up        menerapkan migrasi yang belum dijalankan
//	lentera-server migrate down      membatalkan migrasi terakhir
//	lentera-server migrate status    menampilkan status migrasi
package main

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/jefthah/lentera/server/internal/config"
	"github.com/jefthah/lentera/server/internal/database"
	"github.com/jefthah/lentera/server/internal/httpapi"
	"github.com/jefthah/lentera/server/internal/mail"
)

func main() {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, nil))

	if err := run(os.Args[1:], logger); err != nil {
		logger.Error("lentera-server berhenti", "error", err)
		os.Exit(1)
	}
}

func run(args []string, logger *slog.Logger) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	command := "serve"
	if len(args) > 0 {
		command = args[0]
	}

	switch command {
	case "serve":
		return serve(ctx, cfg, logger)
	case "migrate":
		migration := "up"
		if len(args) > 1 {
			migration = args[1]
		}
		pool, err := database.Connect(ctx, cfg.DatabaseURL)
		if err != nil {
			return err
		}
		defer pool.Close()
		return database.Migrate(ctx, pool, migration, os.Stdout)
	default:
		return fmt.Errorf("perintah %q tidak dikenal; pakai serve atau migrate up|down|status", command)
	}
}

func serve(ctx context.Context, cfg config.Config, logger *slog.Logger) error {
	pool, err := database.Connect(ctx, cfg.DatabaseURL)
	if err != nil {
		return err
	}
	defer pool.Close()

	settings := httpapi.Settings{LoginCodeSecret: cfg.LoginCodeSecret, Mailer: mail.LogMailer{Logger: logger}}
	if cfg.ResendAPIKey != "" {
		settings.Mailer = mail.NewResendMailer(cfg.ResendAPIKey, cfg.MailFrom)
	} else {
		logger.Warn("RESEND_API_KEY kosong: kode masuk hanya ditulis ke log, tidak dikirim lewat email")
	}
	api := httpapi.NewServer(pool, logger, settings)

	server := &http.Server{
		Addr:              cfg.HTTPAddress,
		Handler:           httpapi.NewHandler(api),
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       120 * time.Second,
		WriteTimeout:      120 * time.Second,
		IdleTimeout:       120 * time.Second,
	}

	go removeExpiredSessions(ctx, api, logger)

	serveErrors := make(chan error, 1)
	go func() {
		logger.Info("API berjalan", "address", cfg.HTTPAddress, "environment", cfg.Environment)
		serveErrors <- server.ListenAndServe()
	}()

	select {
	case err := <-serveErrors:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	case <-ctx.Done():
	}

	logger.Info("menghentikan API")
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()

	return server.Shutdown(shutdownCtx)
}

// removeExpiredSessions clears expired sessions and codes every hour until ctx ends.
func removeExpiredSessions(ctx context.Context, api *httpapi.Server, logger *slog.Logger) {
	ticker := time.NewTicker(time.Hour)
	defer ticker.Stop()
	for {
		if err := api.Auth().RemoveExpired(ctx); err != nil && ctx.Err() == nil {
			logger.Warn("membersihkan sesi kedaluwarsa gagal", "error", err)
		}
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}
