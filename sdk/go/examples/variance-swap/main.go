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
	today, err := itofin.NewDate(5, 10, 2026)
	if err != nil {
		return err
	}
	maturity, err := today.AddDays(365)
	if err != nil {
		return err
	}
	settings, err := session.NewSettings()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, settings.Close()) }()
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	dc, err := session.Actual365Fixed()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, dc.Close()) }()
	process, err := session.NewBlackScholesProcess(itofin.BlackScholesConfig{
		Spot: 100, RiskFreeRate: 0.03, DividendYield: 0, Volatility: 0.20,
		ReferenceDate: today, DayCounter: dc,
	})
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, process.Close()) }()
	var calls, puts []float64
	for strike := 100; strike <= 150; strike += 5 {
		calls = append(calls, float64(strike))
	}
	for strike := 50; strike <= 100; strike += 5 {
		puts = append(puts, float64(strike))
	}
	engine, err := session.NewReplicatingVarianceSwapEngine(itofin.ReplicatingVarianceSwapEngineConfig{
		Process: process, Dk: 5, CallStrikes: calls, PutStrikes: puts,
	})
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, engine.Close()) }()
	swap, err := session.NewVarianceSwap(itofin.VarianceSwapConfig{
		Position: itofin.PositionLong, Strike: 0.04, Notional: 50_000,
		StartDate: today, MaturityDate: maturity, Settings: settings,
	})
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, swap.Close()) }()
	if err = swap.SetEngine(engine); err != nil {
		return err
	}
	variance, err := swap.Variance()
	if err != nil {
		return err
	}
	npv, err := swap.NPV()
	if err != nil {
		return err
	}
	weights, err := swap.OptionWeights()
	if err != nil {
		return err
	}
	fmt.Printf("annualized variance: %.10f\nNPV: %.10f\npurchased option weights: %d\n", variance, npv, len(weights))
	return nil
}
