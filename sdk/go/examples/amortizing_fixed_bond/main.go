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
	session := must(itofin.NewSession())
	defer session.Close()
	settings := must(session.NewSettings())
	today := must(itofin.NewDate(20, 1, 2026))
	if err := settings.SetEvaluationDate(today); err != nil {
		log.Fatal(err)
	}
	calendar := must(session.WeekendsOnly())
	dc := must(session.Actual360())
	schedule := must(session.NewSchedule(itofin.ScheduleConfig{Start: must(itofin.NewDate(17, 1, 2024)), End: must(itofin.NewDate(17, 1, 2027)), Frequency: itofin.Annual, Calendar: calendar, Convention: itofin.Unadjusted}))
	bond := must(session.NewAmortizingFixedRateBond(itofin.AmortizingFixedRateBondConfig{SettlementDays: 2, Notionals: []float64{1000, 750, 400}, Schedule: schedule, Coupons: []float64{.045, .052, .039}, AccrualDayCounter: dc, Settings: settings}))
	curve := must(session.NewFlatForward(today, .031, dc))
	engine := must(session.NewDiscountingBondEngine(curve, settings, nil))
	if err := bond.SetEngine(engine); err != nil {
		log.Fatal(err)
	}
	fmt.Println("Outstanding principal:", must(bond.Notional(nil)))
	fmt.Println("Dirty price per100:", must(bond.DirtyPrice()))
	fmt.Println("Currency settlement value:", must(bond.SettlementValue()))
	principal := must(bond.Redemptions())
	defer principal.Close()
	for i := 0; i < must(principal.Len()); i++ {
		flow := must(principal.At(i))
		fmt.Println("Principal:", must(flow.Date()), must(flow.Amount()))
		if err := flow.Close(); err != nil {
			log.Fatal(err)
		}
	}
}
