package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	rows := [][]float64{{1, 4}, {3, 2}, {100, -20}}
	weights := []float64{1, 3, 0}
	mean, err := itofin.StatisticsSequenceMean(rows, weights)
	if err != nil {
		panic(err)
	}
	covariance, err := itofin.StatisticsCovariance(rows, weights)
	if err != nil {
		panic(err)
	}
	correlation, err := itofin.StatisticsCorrelation(rows, weights)
	if err != nil {
		panic(err)
	}
	maximum, err := itofin.StatisticsSequenceMaximum(rows, weights)
	if err != nil {
		panic(err)
	}
	if !closeVector(mean, []float64{2.5, 2.5}) ||
		!closeVector(covariance[0], []float64{1.125, -1.125}) ||
		!closeVector(covariance[1], []float64{-1.125, 1.125}) ||
		!closeVector(correlation[0], []float64{1, -1}) ||
		!closeVector(correlation[1], []float64{-1, 1}) ||
		!closeVector(maximum, []float64{100, 4}) {
		panic("unexpected sequence-statistics result")
	}
	fmt.Println("mean:", mean)
	fmt.Println("covariance:", covariance)
	fmt.Println("correlation:", correlation)
	fmt.Println("maximum:", maximum)
}

func closeVector(actual, expected []float64) bool {
	if len(actual) != len(expected) {
		return false
	}
	for i, value := range actual {
		if math.IsNaN(value) || math.Abs(value-expected[i]) > 1e-12 {
			return false
		}
	}
	return true
}
