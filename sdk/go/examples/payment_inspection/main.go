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
	s := must(itofin.NewSession())
	defer s.Close()
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(must(itofin.NewDate(20, 1, 2026))); err != nil {
		panic(err)
	}
	cal := must(s.NullCalendar())
	dc := must(s.Actual365Fixed())
	schedule := must(s.NewSchedule(itofin.ScheduleConfig{Start: must(itofin.NewDate(15, 1, 2025)), End: must(itofin.NewDate(15, 1, 2027)), Frequency: itofin.Semiannual, Calendar: cal, Convention: itofin.Unadjusted}))
	bond := must(s.NewFixedRateBond(itofin.FixedRateBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Settings: settings}, Schedule: schedule, Coupons: []float64{.045}, AccrualDayCounter: dc}))
	fmt.Println("Accrual starts:", must(bond.StartDate()))
	fmt.Println("Tradable:", must(bond.IsTradable(nil)))
	date, amount := must(bond.NextCashFlowDate(nil)), must(bond.NextCashFlowAmount(nil))
	if date != nil && amount != nil {
		fmt.Println("Next actual payment:", *date, *amount)
	}
	leg := must(bond.Cashflows())
	previous := must(leg.PreviousCashFlowAmount(itofin.CashFlowsPaymentConfig{Settings: settings}))
	if previous != nil {
		fmt.Println("Previous actual payment:", *previous)
	}
}
