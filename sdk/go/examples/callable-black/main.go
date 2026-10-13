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
	volatility := must(s.NewSimpleQuote(.3))
	engine := must(s.NewBlackCallableFixedRateBondEngine(volatility, curve, settings))
	call := must(itofin.NewCallability(100, itofin.Clean, itofin.CallabilityCall, must(itofin.NewDate(7, 7, 2028))))
	bond := must(s.NewCallableZeroCouponBond(itofin.CallableZeroCouponBondConfig{
		ZeroCouponBondConfig: itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: cal, Settings: settings}, MaturityDate: must(itofin.NewDate(7, 7, 2031))},
		DayCounter:           dc, Callability: itofin.CallabilitySchedule{call},
	}))
	if err := bond.SetBlackEngine(engine); err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Black single-European callable NPV: %.8f\n", must(bond.NPV()))
	price := must(bond.CleanPrice())
	fmt.Printf("Settlement clean price per 100: %.8f\n", price)
	fmt.Printf("Relative forward-yield IV: %.8f\n", must(bond.ImpliedVolatility(price, curve, nil)))
}
