package main

import (
	"fmt"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func check(err error) {
	if err != nil {
		panic(err)
	}
}

func main() {
	s := must(itofin.NewSession())
	defer func() { check(s.Close()) }()
	today := must(itofin.NewDate(15, 6, 2026))
	settings := must(s.NewSettings())
	check(settings.SetEvaluationDate(today))
	dc := must(s.Actual360())
	process := must(s.NewBlackScholesProcess(itofin.BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	grid, damping := uint(400), uint(2)
	engine := must(s.NewFdBlackScholesVanillaEngine(process, itofin.FdConfig{TGrid: &grid, XGrid: &grid, DampingSteps: &damping}))
	option := must(s.NewGapOption(itofin.Call, 100, 160, must(today.AddDays(360)), settings))
	fmt.Printf("Signed gap NPV (FD): %.8f\n", must(option.PriceFd(engine)))
}
