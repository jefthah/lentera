// Package database connects to PostgreSQL and applies the schema migrations embedded in the binary.
package database

import (
	"context"
	"embed"
	"fmt"
	"io"
	"io/fs"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
)

//go:embed migrations/*.sql
var embeddedMigrations embed.FS

// Connect opens a connection pool and fails fast when the database cannot be reached.
func Connect(ctx context.Context, databaseURL string) (*pgxpool.Pool, error) {
	pool, err := pgxpool.New(ctx, databaseURL)
	if err != nil {
		return nil, fmt.Errorf("membaca DATABASE_URL: %w", err)
	}

	pingCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	if err := pool.Ping(pingCtx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("menghubungi database: %w", err)
	}

	return pool, nil
}

// Migrate runs a goose command ("up", "down", or "status") and writes one line per migration to out.
func Migrate(ctx context.Context, pool *pgxpool.Pool, command string, out io.Writer) error {
	migrations, err := fs.Sub(embeddedMigrations, "migrations")
	if err != nil {
		return err
	}

	db := stdlib.OpenDBFromPool(pool)
	defer db.Close()

	provider, err := goose.NewProvider(goose.DialectPostgres, db, migrations)
	if err != nil {
		return fmt.Errorf("menyiapkan migrasi: %w", err)
	}

	switch command {
	case "up":
		results, err := provider.Up(ctx)
		for _, result := range results {
			fmt.Fprintf(out, "naik  %05d %s (%s)\n", result.Source.Version, result.Source.Path, result.Duration.Round(time.Millisecond))
		}
		if err == nil && len(results) == 0 {
			fmt.Fprintln(out, "skema sudah terbaru")
		}
		return err
	case "down":
		result, err := provider.Down(ctx)
		if result != nil {
			fmt.Fprintf(out, "turun %05d %s (%s)\n", result.Source.Version, result.Source.Path, result.Duration.Round(time.Millisecond))
		}
		return err
	case "status":
		statuses, err := provider.Status(ctx)
		for _, status := range statuses {
			fmt.Fprintf(out, "%-8s %05d %s\n", status.State, status.Source.Version, status.Source.Path)
		}
		return err
	default:
		return fmt.Errorf("perintah migrasi %q tidak dikenal; pakai up, down, atau status", command)
	}
}
