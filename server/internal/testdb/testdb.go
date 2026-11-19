// Package testdb gives each integration test its own throwaway PostgreSQL database with the
// Lentera schema.
package testdb

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"io"
	"os"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/jefthah/lentera/server/internal/database"
)

// New creates a migrated database and drops it when the test finishes.
// The test is skipped when TEST_DATABASE_URL is not set.
func New(t *testing.T) *pgxpool.Pool {
	t.Helper()

	baseURL := os.Getenv("TEST_DATABASE_URL")
	if baseURL == "" {
		t.Skip("TEST_DATABASE_URL belum diisi; tes integrasi dilewati")
	}

	ctx := context.Background()
	admin, err := pgx.Connect(ctx, baseURL)
	if err != nil {
		t.Fatalf("menghubungi TEST_DATABASE_URL: %v", err)
	}

	name := "lentera_test_" + randomSuffix(t)
	if _, err := admin.Exec(ctx, "CREATE DATABASE "+name); err != nil {
		admin.Close(ctx)
		t.Fatalf("membuat database tes: %v", err)
	}

	poolConfig, err := pgxpool.ParseConfig(baseURL)
	if err != nil {
		t.Fatalf("membaca TEST_DATABASE_URL: %v", err)
	}
	poolConfig.ConnConfig.Database = name

	pool, err := pgxpool.NewWithConfig(ctx, poolConfig)
	if err != nil {
		t.Fatalf("membuka database tes: %v", err)
	}

	t.Cleanup(func() {
		pool.Close()
		if _, err := admin.Exec(context.Background(), "DROP DATABASE IF EXISTS "+name+" WITH (FORCE)"); err != nil {
			t.Errorf("menghapus database tes %s: %v", name, err)
		}
		admin.Close(context.Background())
	})

	if err := database.Migrate(ctx, pool, "up", io.Discard); err != nil {
		t.Fatalf("migrasi database tes: %v", err)
	}

	return pool
}

func randomSuffix(t *testing.T) string {
	t.Helper()

	bytes := make([]byte, 6)
	if _, err := rand.Read(bytes); err != nil {
		t.Fatalf("membuat nama database tes: %v", err)
	}

	return hex.EncodeToString(bytes)
}
