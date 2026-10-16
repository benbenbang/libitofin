package main

import (
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		log.Fatal(err)
	}
	return value
}
func main() {
	s := must(itofin.NewSession())
	defer s.Close()
	today := must(itofin.NewDate(7, 7, 2026))
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(today); err != nil {
		log.Fatal(err)
	}
	cal := must(s.NullCalendar())
	dc := must(s.Actual365Fixed())
	curve := must(s.NewFlatForward(today, .03, dc))
	model := must(s.NewHullWhite(curve, .1, .01))
	engine := must(s.NewTreeCallableFixedRateBondEngine(model, 100, settings))
	call := must(itofin.NewCallability(100, itofin.Clean, itofin.CallabilityCall, must(itofin.NewDate(7, 7, 2028))))
	bond := must(s.NewCallableZeroCouponBond(itofin.CallableZeroCouponBondConfig{
		ZeroCouponBondConfig: itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: cal, Settings: settings}, MaturityDate: must(itofin.NewDate(7, 7, 2031))},
		DayCounter:           dc, Callability: itofin.CallabilitySchedule{call},
	}))
	fmt.Printf("Effective clean-price duration: %.8f\n", must(bond.EffectiveDuration(.0075, engine, dc, itofin.Continuous, itofin.Annual, nil)))
	fmt.Printf("Effective clean-price convexity: %.8f\n", must(bond.EffectiveConvexity(.0075, engine, dc, itofin.Continuous, itofin.Annual, nil)))
}
