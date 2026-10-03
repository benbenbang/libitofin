package main

import (
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	paths, err := itofin.SimulateGJR(itofin.GJRConfig{
		Spot: 100, DailyVariance: .00016, RiskFreeRate: .05, DividendYield: .02,
		Omega: .000002, Alpha: .04, Beta: .9, Gamma: .06, Lambda: .1, DaysPerYear: 252,
		Horizon: 1, Steps: 252, Paths: 3, Seed: 42,
		Scheme: itofin.GJRFullTruncation, TerminalOnly: true,
	})
	if err != nil {
		log.Fatal(err)
	}
	for path := range paths.Paths {
		fmt.Printf("path %d: spot %.8f, annualized variance %.8f\n", path, paths.Values[path*2], paths.Values[path*2+1])
	}
}
