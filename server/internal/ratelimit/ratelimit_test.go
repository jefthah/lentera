package ratelimit

import (
	"testing"
	"time"
)

func TestWindowLimitsAndResets(t *testing.T) {
	now := time.Unix(1_800_000_000, 0)
	limiter := New()
	limiter.now = func() time.Time { return now }

	for range 3 {
		if ok, _ := limiter.Allow("a", 3, time.Minute); !ok {
			t.Fatal("attempt within limit was refused")
		}
	}
	ok, wait := limiter.Allow("a", 3, time.Minute)
	if ok || wait != time.Minute {
		t.Fatalf("fourth attempt: ok=%v wait=%v", ok, wait)
	}
	if ok, _ := limiter.Allow("b", 3, time.Minute); !ok {
		t.Fatal("other key was refused")
	}

	now = now.Add(time.Minute)
	if ok, _ := limiter.Allow("a", 3, time.Minute); !ok {
		t.Fatal("attempt after the window was refused")
	}
}
