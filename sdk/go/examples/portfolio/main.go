// This example uses synthetic allocations; no service-specific code or data.
package main

import (
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	result, err := itofin.SimulateGBM(itofin.GBMConfig{
		Initial:     []float64{6000, 4000},
		Drift:       []float64{0.06, 0.03},
		Volatility:  []float64{0.20, 0.10},
		Correlation: []float64{1, 0.3, 0.3, 1},
		Horizon:     1, Steps: 252, Paths: 1000, Seed: 42, TerminalOnly: true,
	})
	if err != nil {
		log.Fatal(err)
	}
	var mean float64
	for path := range result.Paths {
		var total float64
		for asset := range result.Assets {
			total += result.Values[path*result.Assets+asset]
		}
		mean += total / float64(result.Paths)
	}
	fmt.Printf("native %s; %d paths; mean terminal portfolio %.2f\n", itofin.Version(), result.Paths, mean)
}
