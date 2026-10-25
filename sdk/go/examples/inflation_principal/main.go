package main

import (
	"fmt"
	f "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}
func day(d, m, y int) f.Date { return must(f.NewDate(d, m, y)) }
func main() {
	s := must(f.NewSession())
	defer s.Close()
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(day(18, 8, 2025)); err != nil {
		panic(err)
	}
	index := must(s.NewUKRPI(settings))
	for _, fixing := range []struct {
		date  f.Date
		value float64
	}{{day(1, 1, 2024), 90}, {day(1, 7, 2024), 110}} {
		if err := index.AddFixing(fixing.date, fixing.value); err != nil {
			panic(err)
		}
	}
	fixed := must(s.NewCPICashFlow(f.CPICashFlowConfig{Index: index, Nominal: 1000, BaseCPI: 100, ObservationDate: day(15, 7, 2024), PaymentDate: day(15, 7, 2026), ObservationLag: f.Period{Length: 0, Unit: f.Months}, Interpolation: f.CpiFlat, GrowthOnly: true}))
	live := must(s.NewZeroInflationCashFlow(f.ZeroInflationCashFlowConfig{Index: index, Nominal: 1000, StartDate: day(15, 1, 2024), EndDate: day(15, 7, 2024), PaymentDate: day(15, 7, 2026), ObservationLag: f.Period{Length: 0, Unit: f.Months}, Interpolation: f.CpiFlat, GrowthOnly: true}))
	dc := must(s.Actual365Fixed())
	discount := must(s.NewFlatForward(day(18, 8, 2025), .04, dc))
	for _, flow := range []*f.InflationCashFlow{fixed, live} {
		leg := must(flow.AsLeg())
		fmt.Println("amount", must(flow.Amount()), "NPV", must(leg.NPV(f.CashFlowsNPVConfig{Discount: discount, Settings: settings})))
	}
}
