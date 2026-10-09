package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"math"
)

func main() {
	asset := []float64{-0.01, 0.02, 0.05, 0.03}
	benchmark := []float64{-0.02, 0, 0.01, 0.04}
	beta, err := itofin.BenchmarkBeta(asset, benchmark, []float64{1, 2, 3, 0})
	if err != nil {
		panic(err)
	}
	if math.Abs(beta-84.0/41.0) > 2e-12 {
		panic("unexpected beta")
	}
	fmt.Printf("Benchmark beta: %.8f\n", beta)
}
