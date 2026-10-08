package main

import (
	"context"
	"fmt"
	"log"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() error {
	xatol, fatol := 1e-7, 1e-10
	calls := 0
	objective := func(x []float64) (float64, error) {
		calls++
		return math.Pow(x[0]-1.25, 2) + 4*math.Pow(x[1]+0.75, 2) - 3, nil
	}
	result, err := itofin.Minimize(context.Background(), objective, []float64{-3, 3},
		itofin.Firefly{
			Bounds: [][2]float64{{-4, 4}, {-4, 4}},
			Seed:   42, PopulationSize: 24, XAtol: &xatol, FAtol: &fatol,
		})
	if err != nil {
		return err
	}
	if !result.Success || math.Abs(result.X[0]-1.25) >= 1e-5 ||
		math.Abs(result.X[1]+0.75) >= 1e-5 || math.Abs(result.Fun+3) >= 1e-8 || result.Njev != 0 || result.Nfev != calls {
		return fmt.Errorf("unexpected analytic result: %+v", result)
	}
	fmt.Printf("x=%v, scalar minimum=%.10f\n", result.X, result.Fun)
	fmt.Printf("generations=%d, objective calls=%d\n", result.Nit, result.Nfev)
	fmt.Println(result.Status, result.Message)
	return nil
}
