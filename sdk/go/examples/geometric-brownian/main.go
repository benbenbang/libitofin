package main

import (
	"errors"
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() (err error) {
	session, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, session.Close()) }()
	process, err := session.NewGeometricBrownianMotionProcess(100, 0.05, 0.20)
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, process.Close()) }()
	queries := []struct {
		label string
		get   func() (float64, error)
	}{
		{"initial state", process.X0},
		{"mu", process.Mu},
		{"volatility", process.Volatility},
		{"arithmetic drift", func() (float64, error) { return process.Drift(0, 100) }},
		{"arithmetic diffusion", func() (float64, error) { return process.Diffusion(0, 100) }},
		{"Euler expectation", func() (float64, error) { return process.Expectation(0, 100, 0.25) }},
		{"Euler variance", func() (float64, error) { return process.Variance(0, 100, 0.25) }},
		{"signed Euler deviation", func() (float64, error) { return process.StdDeviation(0, -100, 0.25) }},
		{"Euler transition", func() (float64, error) { return process.Evolve(0, 100, 0.25, -0.25) }},
		{"Euler crossing zero", func() (float64, error) { return process.Evolve(0, 100, 1, -6) }},
	}
	for _, query := range queries {
		value, queryErr := query.get()
		if queryErr != nil {
			return queryErr
		}
		fmt.Printf("%s: %.10f\n", query.label, value)
	}
	return nil
}
