package feel

import "strconv"

// Go's JSON decoder replaces lone UTF-16 escapes; FEEL admission must reject
// them, rather than changing the literal before compiling it.
func validSurrogates(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] != '\\' {
			continue
		}
		i++
		if i >= len(s) {
			return false
		}
		if s[i] != 'u' {
			continue
		}
		if i+4 >= len(s) {
			return false
		}
		v, e := strconv.ParseUint(s[i+1:i+5], 16, 16)
		if e != nil {
			return false
		}
		i += 4
		if v >= 0xdc00 && v <= 0xdfff {
			return false
		}
		if v >= 0xd800 && v <= 0xdbff {
			if i+6 >= len(s) || s[i+1] != '\\' || s[i+2] != 'u' {
				return false
			}
			low, e := strconv.ParseUint(s[i+3:i+7], 16, 16)
			if e != nil || low < 0xdc00 || low > 0xdfff {
				return false
			}
			i += 6
		}
	}
	return true
}
