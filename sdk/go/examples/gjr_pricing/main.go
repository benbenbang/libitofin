package main

import (
	"errors"
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() (err error) {
	s, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, s.Close()) }()
	today := must(itofin.NewDate(3, 10, 2026))
	expiry := must(today.AddDays(365))
	settings := must(s.NewSettings())
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	dc := must(s.Actual365Fixed())
	spot := must(s.NewSimpleQuote(100))
	risk := must(s.NewFlatForward(today, .05, dc))
	dividend := must(s.NewFlatForward(today, .02, dc))
	params := itofin.GJRParameters{DailyVariance: .00016, Omega: .000002, Alpha: .04, Beta: .9, Gamma: .06, Lambda: .1, DaysPerYear: 252}
	process := must(s.NewGJRProcess(spot, risk, dividend, params, itofin.GJRFullTruncation))
	model := must(s.NewGJRModel(process))
	analytic := must(s.NewAnalyticGJREngine(model))
	option := must(s.NewVanillaOption(itofin.Call, 100, expiry, settings))
	fmt.Println("Analytic GJR NPV:", must(option.PriceAnalyticGJR(analytic)))
	mc := must(s.NewMCGJREngine(process, itofin.GJRMCConfig{StepsPerYear: 252, Samples: 4096, Seed: 42, Antithetic: true}))
	fmt.Println("Monte Carlo GJR NPV:", must(option.PriceMCGJR(mc)))
	fmt.Println("Monte Carlo standard error:", must(option.ErrorEstimate()))
	fmt.Println("Daily omega,alpha,beta,gamma,lambda,v0:", must(model.Params()))
	if err = spot.SetValue(105); err != nil {
		return err
	}
	fmt.Println("After live spot update:", must(option.NPV()))
	return nil
}
