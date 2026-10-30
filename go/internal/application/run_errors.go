package application

// ExtractionFailure is the safe, stable boundary for response parser and FEEL
// failures. Parser messages and property names must never become public errors.
func ExtractionFailure(kind string) *Failure {
	message, ok := extractionMessages[kind]
	if !ok {
		kind = "unknownExtraction"
		message = "Compiled extraction is unavailable."
	}
	return &Failure{Kind: kind, Message: message}
}

var extractionMessages = map[string]string{
	"unknownExtraction":             "Compiled extraction is unavailable.",
	"extractionResponseUnavailable": "Required response body is unavailable.",
	"extractionResponseTruncated":   "Extraction requires a complete, untruncated response body.",
	"extractionResponseLimit":       "Response data exceeds the per-response or aggregate byte limit.",
	"extractionInvalidJson":         "Response body must contain exactly one valid JSON value.",
	"extractionDuplicateKey":        "Response JSON contains a duplicate object key.",
	"extractionJsonDepthLimit":      "Response JSON exceeds the nesting limit.",
	"extractionJsonValueLimit":      "Response JSON exceeds the value-count limit.",
	"extractionMissingProperty":     "Required response property is missing.",
	"extractionNullValue":           "Required response property is null.",
	"extractionWrongType":           "Extraction requires a scalar matching its declared type.",
	"extractionEmptyValue":          "Extracted string must not be empty.",
	"extractionValueLimit":          "Extracted string exceeds the 8192-byte limit.",
	"extractionOutputLimit":         "Extraction outputs exceed the entry or aggregate byte limit.",
	"extractionOutputUnavailable":   "Required extraction output is unavailable.",
}

var runtimeMessages = map[string]string{
	"resourceLimit":          "Run report byte limit reached; execution stopped",
	"valueLimit":             "Aggregate invocation storage exceeds its resource budget.",
	"invocationLimit":        "Workflow invocation limit exceeded.",
	"routeNoMatch":           "No route condition matched and no default exists.",
	"routeAmbiguous":         "More than one route condition matched.",
	"routeSourceUnavailable": "A route property or input is unavailable.",
	"routeTypeMismatch":      "Route operands must have matching scalar types; ordering requires integers or strings.",
}
