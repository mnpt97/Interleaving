//go:build !darwin || ios || server

package wails

import (
	"errors"

	native "github.com/wailsapp/wails/v3/pkg/application"
)

func applyNativeTheme(_ *native.WebviewWindow, theme string) error {
	if theme == "system" {
		return nil
	}
	return errors.New("native theme override has not been implemented on this platform")
}
