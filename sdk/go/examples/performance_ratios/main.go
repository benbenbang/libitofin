package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	returns := []float64{0.02, -0.01, 0.03, -0.02}
	downside, err := itofin.TargetDownsideDeviation(returns, 0)
	if err != nil {
		panic(err)
	}
	sharpe, err := itofin.SharpeRatio(returns, 0, 12)
	if err != nil {
		panic(err)
	}
	sortino, err := itofin.SortinoRatio(returns, 0, 12)
	if err != nil {
		panic(err)
	}
	if math.Abs(downside-math.Sqrt(0.0005/4)) > 1e-14 || math.Abs(sortino-math.Sqrt(12.0/5)) > 1e-13 {
		panic("performance ratio self-check failed")
	}
	fmt.Printf("Monthly downside: %.8f; annualized Sharpe: %.8f; Sortino: %.8f\n", downside, sharpe, sortino)
}
