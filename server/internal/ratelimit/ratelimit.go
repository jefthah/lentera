// Package ratelimit counts requests per key in fixed time windows, in memory. The Lentera server
// runs as a single process, so the counters do not need to be shared.
package ratelimit

import (
	"sync"
	"time"
)

// Limiter counts attempts per key. The zero value is not usable; call New.
type Limiter struct {
	mu      sync.Mutex
	now     func() time.Time
	windows map[string]window
	calls   int
}

type window struct {
	count  int
	resets time.Time
}

// New creates an empty limiter.
func New() *Limiter {
	return &Limiter{now: time.Now, windows: make(map[string]window)}
}

// Allow records one attempt for key. It reports whether the attempt is within limit for the
// current window and, when it is not, how long until the window resets.
func (l *Limiter) Allow(key string, limit int, period time.Duration) (bool, time.Duration) {
	l.mu.Lock()
	defer l.mu.Unlock()

	now := l.now()
	l.calls++
	if l.calls%1000 == 0 {
		l.removeExpired(now)
	}

	current, ok := l.windows[key]
	if !ok || !now.Before(current.resets) {
		current = window{resets: now.Add(period)}
	}
	if current.count >= limit {
		return false, current.resets.Sub(now)
	}

	current.count++
	l.windows[key] = current

	return true, 0
}

func (l *Limiter) removeExpired(now time.Time) {
	for key, current := range l.windows {
		if !now.Before(current.resets) {
			delete(l.windows, key)
		}
	}
}
