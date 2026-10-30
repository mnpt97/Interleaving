// Package domain defines framework-independent product capabilities and failures.
package domain

import "errors"

// ErrUnavailable means a migration capability has not been implemented, not that input is valid.
var ErrUnavailable = errors.New("requested Go capability is not implemented yet")

// Capabilities describes implemented behavior; a shell probe is never a scenario run.
type Capabilities struct {
	Validation  bool `json:"validation"`
	Execution   bool `json:"execution"`
	BridgeProbe bool `json:"bridgeProbe"`
}
