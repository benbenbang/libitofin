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
	today := must(itofin.NewDate(15, 1, 2024))
	maturity := must(itofin.NewDate(15, 1, 2027))
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(today); err != nil {
		log.Fatal(err)
	}
	cal := must(s.NullCalendar())
	dc := must(s.Actual365Fixed())
	call := must(itofin.NewCallability(100, itofin.Clean, itofin.CallabilityCall, must(itofin.NewDate(15, 1, 2025))))
	bond := must(s.NewCallableZeroCouponBond(itofin.CallableZeroCouponBondConfig{
		ZeroCouponBondConfig: itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: cal, Settings: settings}, MaturityDate: maturity},
		DayCounter:           dc, Callability: itofin.CallabilitySchedule{call},
	}))
	fmt.Printf("Exercise rights: %d\n", len(must(bond.Callability())))
	fmt.Printf("Contractual cashflow clean price at 4%% yield: %.6f\n", must(bond.CleanPriceFromYield(.04, itofin.BondYieldConfig{DayCounter: dc, Compounding: itofin.Compounded, Frequency: itofin.Annual})))
}
