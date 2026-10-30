// Package system supplies operating-system implementations of application ports.
package system

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"time"
)

type Timers struct{}

func (Timers) Timeout(parent context.Context, duration time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(parent, duration)
}

type Clock struct{}

func (Clock) Now() time.Time { return time.Now().UTC() }

type IDs struct{}

func (IDs) NewID() string {
	var id [16]byte
	if _, err := rand.Read(id[:]); err != nil {
		panic("system random source unavailable")
	}
	return hex.EncodeToString(id[:])
}
