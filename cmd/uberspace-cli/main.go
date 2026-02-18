package main

import (
	"os"

	"uberspace-cli/internal/cli"
)

func main() {
	c := cli.New()
	os.Exit(c.Run(os.Args[1:]))
}
