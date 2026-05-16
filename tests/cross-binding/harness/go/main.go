// Go harness — parses a .tomlp file via cgo, prints JSON.
package main

import (
	"encoding/json"
	"fmt"
	"os"

	tomlplus "github.com/CarsonKopec/tomlplus/bindings/tomlplus-go"
)

func main() {
	doc, err := tomlplus.Load(os.Args[1])
	if err != nil {
		fmt.Fprintf(os.Stderr, "parse: %v\n", err)
		os.Exit(1)
	}
	defer doc.Close()
	cfg, err := doc.Config()
	if err != nil {
		fmt.Fprintf(os.Stderr, "config: %v\n", err)
		os.Exit(1)
	}
	out, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		fmt.Fprintf(os.Stderr, "marshal: %v\n", err)
		os.Exit(1)
	}
	fmt.Println(string(out))
}
