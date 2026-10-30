// Package bootstrap wires non-desktop dependencies shared by CLI and Wails.
package bootstrap

import (
	"interleaving/internal/adapters/outbound/envcredentials"
	"interleaving/internal/adapters/outbound/feel"
	"interleaving/internal/adapters/outbound/httpclient"
	"interleaving/internal/adapters/outbound/reportfiles"
	"interleaving/internal/adapters/outbound/scenariocodec"
	"interleaving/internal/adapters/outbound/scenariofiles"
	"interleaving/internal/adapters/outbound/system"
	"interleaving/internal/adapters/outbound/targeturls"
	"interleaving/internal/application"
)

func Workspace(logger application.Logger) *application.Workspace {
	codec, err := scenariocodec.New()
	if err != nil {
		panic("invalid embedded scenario schema")
	}
	return application.NewWorkspace(system.Clock{}, system.IDs{}, logger, application.WithScenarios(application.ScenarioServices{Codec: codec, Files: scenariofiles.Adapter{}, Writer: scenariofiles.Adapter{}, Expressions: feel.Adapter{}, Targets: targeturls.Adapter{}}), application.WithRuns(application.RunServices{Sessions: httpclient.Adapter{}, Credentials: envcredentials.Adapter{}, Reports: reportfiles.Adapter{}, Timers: system.Timers{}, JSON: scenariocodec.ResponseJSON{}, URLs: targeturls.Adapter{}}))
}
