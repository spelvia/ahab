package agent

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"time"
)

// NewSessionID returns a sortable, unique session identifier like
// "20260704-171502-9f3a".
func NewSessionID() string {
	var b [2]byte
	_, _ = rand.Read(b[:])
	return fmt.Sprintf("%s-%s", time.Now().Format("20060102-150405"), hex.EncodeToString(b[:]))
}
