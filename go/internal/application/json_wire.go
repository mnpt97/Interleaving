package application

import (
	"bytes"
	"encoding/json"
)

// jsonWire matches scenario HTTP JSON serialization, including UTF-8 and HTML
// characters. Go's default HTML/JavaScript escaping would change request sizes.
func jsonWire(value any) ([]byte, error) {
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(value); err != nil {
		return nil, err
	}
	raw := bytes.TrimSuffix(buffer.Bytes(), []byte{'\n'})
	result := make([]byte, 0, len(raw))
	for i := 0; i < len(raw); i++ {
		if raw[i] == '\\' && i+1 < len(raw) {
			if i+5 < len(raw) && string(raw[i+1:i+5]) == "u202" && (raw[i+5] == '8' || raw[i+5] == '9') {
				result = append(result, 0xe2, 0x80, 0xa8+raw[i+5]-'8')
				i += 5
				continue
			}
			result = append(result, raw[i], raw[i+1])
			i++
			continue
		}
		result = append(result, raw[i])
	}
	return result, nil
}
