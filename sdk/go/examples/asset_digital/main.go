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

func main() {
	session := must(itofin.NewSession())
	defer session.Close()
	today := must(itofin.NewDate(15, 6, 2026))
	settings := must(session.NewSettings())
	if err := settings.SetEvaluationDate(today); err != nil {
		panic(err)
	}
	dc := must(session.Actual360())
	process := must(session.NewBlackScholesProcess(itofin.BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	option := must(session.NewAssetOrNothingOption(itofin.Call, 105, must(today.AddDays(360)), settings))
	grid := uint(400)
	engine := must(session.NewFdBlackScholesVanillaEngine(process, itofin.FdConfig{TGrid: &grid, XGrid: &grid}))
	fmt.Printf("Asset digital FD estimate: %.10f\n", must(option.PriceFd(engine)))
	fmt.Printf("FD delta: %.10f\n", must(option.Delta()))
}
