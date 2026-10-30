// Command interl-go is the headless composition root; it must never import Wails.
package main

import (
	"context"
	"os"
	"os/signal"

	"interleaving/internal/adapters/inbound/cli"
	"interleaving/internal/adapters/outbound/charmlog"
	"interleaving/internal/bootstrap"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	os.Exit(cli.Run(ctx, os.Args[1:], os.Stdout, os.Stderr, bootstrap.Workspace(charmlog.New(os.Stderr))))
}
