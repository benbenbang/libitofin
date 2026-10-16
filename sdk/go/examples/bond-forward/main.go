package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"log"
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
	today := must(itofin.NewDate(2, 1, 2025))
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(today); err != nil {
		log.Fatal(err)
	}
	cal := must(s.NullCalendar())
	dc := must(s.Actual365Fixed())
	curve := must(s.NewFlatForward(today, .03, dc))
	bond := must(s.NewZeroCouponBond(itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 100, Calendar: cal, Settings: settings}, MaturityDate: must(itofin.NewDate(2, 7, 2026))}))
	engine := must(s.NewDiscountingBondEngine(curve, settings, nil))
	if err := bond.SetEngine(engine); err != nil {
		log.Fatal(err)
	}
	forward := must(s.NewBondForward(itofin.BondForwardConfig{Bond: &bond.Bond, ValueDate: today, DeliveryDate: must(itofin.NewDate(2, 7, 2025)), Position: itofin.PositionLong, Strike: 95, DayCounter: dc, Calendar: cal, Convention: itofin.Unadjusted, Discount: curve}))
	fmt.Println(must(forward.FairForwardPrice()), must(forward.CleanForwardPrice()), must(forward.NPV()))
}
