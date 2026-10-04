package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	points := [][]float64{{0.25, 0.25}, {0.75, 0.75}, {0.25, 0.75}, {0.75, 0.25}}
	discrepancy, err := itofin.StatisticsDiscrepancy(points, nil)
	if err != nil {
		panic(err)
	}
	if math.Abs(discrepancy-math.Sqrt(71.0/4608.0)) > 1e-12 {
		panic("unexpected uncentered L2 discrepancy")
	}
	fmt.Println("uncentered L2 discrepancy:", discrepancy)
}
