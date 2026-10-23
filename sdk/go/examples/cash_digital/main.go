package main

import (
	"fmt"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](v T, err error) T {
	if err != nil {
		panic(err)
	}
	return v
}

func main() {
	s := must(itofin.NewSession())
	defer s.Close()
	today := must(itofin.NewDate(15, 6, 2026))
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(today); err != nil {
		panic(err)
	}
	dc := must(s.Actual360())
	process := must(s.NewBlackScholesProcess(itofin.BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	option := must(s.NewCashOrNothingOption(itofin.Call, 105, 12, must(today.AddDays(360)), settings))
	fmt.Printf("Digital NPV: %.10f\n", must(option.Price(process)))
	fmt.Printf("Digital delta: %.10f\n", must(option.Delta()))
}
