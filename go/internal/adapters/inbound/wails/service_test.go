package wails

import "testing"

func TestCloseGuardsSurviveOverlappingFrontendSubscriptions(t *testing.T) {
	var s Service
	s.Ready(true)
	s.Ready(true)
	s.Ready(false)
	if s.ready.Load() != 1 {
		t.Fatal("stale listener disabled active close protection")
	}
	s.Ready(false)
	s.Ready(false)
	if s.ready.Load() != 0 {
		t.Fatal("unbalanced guard count")
	}
}
