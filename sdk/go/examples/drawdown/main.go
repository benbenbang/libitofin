package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	result, err := itofin.MaximumDrawdown([]float64{100, 120, 90, 130})
	if err != nil {
		panic(err)
	}
	if result.Drawdown != 0.25 || result.PeakIndex != 1 || result.TroughIndex != 2 {
		panic("unexpected drawdown")
	}
	fmt.Printf("maximum drawdown %.0f%%, peak %d, trough %d\n", 100*result.Drawdown, result.PeakIndex, result.TroughIndex)
}
