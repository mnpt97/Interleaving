//go:build darwin && !ios && !server

package wails

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa
#import <Cocoa/Cocoa.h>
static void interleavingSetAppearance(void *nativeWindow, int theme) {
 NSWindow *window = (NSWindow *)nativeWindow;
 window.appearance = theme == 0 ? nil : [NSAppearance appearanceNamed:(theme == 1 ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua)];
}
*/
import "C"
import (
	"errors"

	native "github.com/wailsapp/wails/v3/pkg/application"
)

// Only the native adapter knows Cocoa; the application/host port uses plain theme values.
func applyNativeTheme(window *native.WebviewWindow, theme string) error {
	var value C.int
	switch theme {
	case "system":
		value = 0
	case "dark":
		value = 1
	case "light":
		value = 2
	default:
		return errors.New("unknown theme")
	}
	native.InvokeSync(func() { C.interleavingSetAppearance(window.NativeWindow(), value) })
	return nil
}
