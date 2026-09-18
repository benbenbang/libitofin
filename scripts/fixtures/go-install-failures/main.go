package main

import (
	"fmt"
	"os"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	session, err := itofin.NewSession()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := session.Close(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println("native session ready")
}
