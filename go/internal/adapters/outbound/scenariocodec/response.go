package scenariocodec

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"interleaving/internal/application"
	"interleaving/internal/domain"
	"io"
	"unicode/utf8"
)

// ResponseJSON reuses bounded duplicate-aware admission but discards private
// parser paths/messages before crossing the application port.
type ResponseJSON struct{}

func (ResponseJSON) Parse(ctx context.Context, data []byte) (any, *application.Failure) {
	kind := "extractionInvalidJson"
	if len(data) > 1048576 {
		return nil, application.ExtractionFailure("extractionResponseLimit")
	}
	if !utf8.Valid(data) {
		return nil, application.ExtractionFailure(kind)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	budget := budget{limits: DefaultLimits, ctx: ctx, source: data}
	budget.limits.Collection = 100000
	value, err := budget.jsonValue(decoder, "", 0)
	if err == nil {
		if _, end := decoder.Token(); end != io.EOF {
			err = errors.New("trailing")
		}
	}
	if err == nil {
		return value, nil
	}
	if ctx.Err() != nil {
		kind = "cancelled"
	} else {
		var d domain.Diagnostic
		if errors.As(err, &d) {
			switch d.Code {
			case "duplicateKey":
				kind = "extractionDuplicateKey"
			case "resourceLimit", "collectionLimit":
				kind = "extractionJsonValueLimit"
				if budget.values <= 100000 {
					kind = "extractionJsonDepthLimit"
				}
			}
		}
	}
	if kind == "cancelled" {
		return nil, &application.Failure{Kind: kind, Message: "Run cancelled"}
	}
	return nil, application.ExtractionFailure(kind)
}
