package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}
func main() {
	session := must(itofin.NewSession())
	defer session.Close()
	date := must(itofin.NewDate(23, 10, 2026))
	dc := must(session.Actual365Fixed())
	curve := must(session.NewFlatForward(date, .03, dc))
	process := must(session.NewHullWhiteForwardProcess(curve, .1, .02))
	if err := process.SetForwardMeasureTime(5); err != nil {
		panic(err)
	}
	fmt.Printf("mean=%.12f variance=%.12f\n", must(process.Expectation(1, .03, .8)), must(process.Variance(1, .03, .8)))
}
