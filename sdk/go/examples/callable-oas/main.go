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
	settings := must(session.NewSettings())
	reference := must(itofin.NewDate(7, 7, 2026))
	if err := settings.SetEvaluationDate(reference); err != nil {
		panic(err)
	}
	dc := must(session.Actual365Fixed())
	calendar := must(session.NullCalendar())
	curve := must(session.NewFlatForward(reference, 0.03, dc))
	model := must(session.NewHullWhite(curve, 0.1, 0.01))
	tree := must(session.NewTreeCallableFixedRateBondEngine(model, 100, settings))
	bond := must(session.NewCallableZeroCouponBond(itofin.CallableZeroCouponBondConfig{ZeroCouponBondConfig: itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Settings: settings, Calendar: calendar}, MaturityDate: must(itofin.NewDate(7, 7, 2031))}, DayCounter: dc}))
	config := itofin.CallableOASConfig{DayCounter: dc, Compounding: itofin.Compounded, Frequency: itofin.Semiannual}
	price := must(bond.CleanPriceOAS(0.015, tree, config))
	spread := must(bond.OAS(price, tree, itofin.CallableOASSolveConfig{CallableOASConfig: config}))
	fmt.Printf("clean %.10f, OAS %.10f\n", price, spread)
}
