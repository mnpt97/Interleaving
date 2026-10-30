package domain

import (
	"encoding/json"
	"strconv"
)

// Integer accepts only integral JSON numbers with the same safe integer bounds
// as the JavaScript editor. Decimal/exponent spellings remain non-integer JSON.
func Integer(v any) (int64, bool) {
	var n int64
	switch v := v.(type) {
	case json.Number:
		var err error
		n, err = strconv.ParseInt(string(v), 10, 64)
		if err != nil {
			return 0, false
		}
	case int:
		n = int64(v)
	case int64:
		n = v
	case uint64:
		if v > 9007199254740991 {
			return 0, false
		}
		n = int64(v)
	default:
		return 0, false
	}
	return n, n >= -9007199254740991 && n <= 9007199254740991
}
func ScalarType(v any) string {
	switch v := v.(type) {
	case string:
		if len(v) <= 8192 {
			return "string"
		}
	case bool:
		return "boolean"
	default:
		if _, ok := Integer(v); ok {
			return "integer"
		}
	}
	return ""
}
