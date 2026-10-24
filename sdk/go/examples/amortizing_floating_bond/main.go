package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func date(day, month, year int) itofin.Date {
	value, err := itofin.NewDate(day, month, year)
	must(err)
	return value
}
func main() {
	s, err := itofin.NewSession()
	must(err)
	defer s.Close()
	settings, err := s.NewSettings()
	must(err)
	today := date(28, 9, 2026)
	must(settings.SetEvaluationDate(today))
	calendar, err := s.NullCalendar()
	must(err)
	counter, err := s.Actual360()
	must(err)
	currency, err := s.EUR()
	must(err)
	quote, err := s.NewSimpleQuote(0.04)
	must(err)
	forward, err := s.NewFlatForwardFromQuote(today, quote, counter)
	must(err)
	discount, err := s.NewFlatForward(today, 0.025, counter)
	must(err)
	index, err := s.NewIborIndex(itofin.IborIndexConfig{FamilyName: "AmortizingExample", Tenor: itofin.Period{Length: 6, Unit: itofin.Months}, SettlementDays: 2, Currency: currency, FixingCalendar: calendar, Convention: itofin.Following, DayCounter: counter, Forwarding: forward, Settings: settings})
	must(err)
	schedule, err := s.NewSchedule(itofin.ScheduleConfig{Start: date(5, 10, 2026), End: date(5, 10, 2027), Frequency: itofin.Quarterly, Calendar: calendar, Convention: itofin.Unadjusted})
	must(err)
	bond, err := s.NewAmortizingFloatingRateBond(itofin.AmortizingFloatingRateBondConfig{Notionals: []float64{1000, 800, 800, 500}, Schedule: schedule, Index: index, AccrualDayCounter: counter, Settings: settings, Spreads: []float64{0.001}})
	must(err)
	engine, err := s.NewDiscountingBondEngine(discount, settings, nil)
	must(err)
	must(bond.SetEngine(engine))
	initial, err := bond.NPV()
	must(err)
	must(quote.SetValue(0.05))
	updated, err := bond.NPV()
	must(err)
	if math.IsNaN(initial) || math.IsInf(initial, 0) || math.IsNaN(updated) || math.IsInf(updated, 0) || updated <= initial {
		panic("live forwarding update failed")
	}
	fmt.Printf("NPV before/after forwarding update: %.10f / %.10f\n", initial, updated)
}
