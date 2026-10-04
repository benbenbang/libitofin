package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	session, err := itofin.NewSession()
	if err != nil {
		panic(err)
	}
	defer session.Close()
	statistics, err := session.NewConvergenceStatistics()
	if err != nil {
		panic(err)
	}
	defer statistics.Close()
	if err := statistics.AddBatch([]float64{2, 100, 8, 12}, []float64{1, 0, 3, 2}); err != nil {
		panic(err)
	}
	table, err := statistics.Table()
	if err != nil {
		panic(err)
	}
	mean, err := statistics.Mean()
	if err != nil {
		panic(err)
	}
	if len(table) != 2 || table[0].Samples != 1 || table[0].Mean != 2 ||
		table[1].Samples != 3 || table[1].Mean != 6.5 || math.Abs(mean-25.0/3.0) > 1e-12 {
		panic("unexpected mean-convergence result")
	}
	fmt.Println("checkpoint table:", table)
	fmt.Println("current mean:", mean)
	if err := statistics.Reset(); err != nil {
		panic(err)
	}
	table, err = statistics.Table()
	if err != nil || len(table) != 0 {
		panic("reset did not clear checkpoints")
	}
}
