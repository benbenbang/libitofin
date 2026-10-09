package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	result, err := itofin.ADX(
		[]float64{10, 12, 11, 14}, []float64{8, 9, 7, 10},
		[]float64{9, 11, 8, 13}, 2,
	)
	if err != nil {
		panic(err)
	}
	if result.DX.FirstValid != 2 || result.ADX.FirstValid != 3 || math.Abs(result.ADX.Values[3]-30) > 1e-12 {
		panic("unexpected ADX fixture")
	}
	fmt.Printf("ADX: %v, warmup: %d\n", result.ADX.Values, result.ADX.FirstValid)
	if result.ADX.NullableValues()[0] != nil {
		panic("warmup must be missing")
	}
}
