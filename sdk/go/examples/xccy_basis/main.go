package main

import (
	"fmt"
	"github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](v T, err error) T {
	if err != nil {
		panic(err)
	}
	return v
}
func main() {
	s := must(itofin.NewSession())
	defer s.Close()
	settings := must(s.NewSettings())
	ref := must(itofin.NewDate(5, 1, 2026))
	if err := settings.SetEvaluationDate(ref); err != nil {
		panic(err)
	}
	dc := must(s.Actual360())
	calendar := must(s.NullCalendar())
	eur := must(s.EUR())
	usd := must(s.USD())
	bc := must(s.NewFlatForward(ref, .03, dc))
	qc := must(s.NewFlatForward(ref, .04, dc))
	bi := must(s.NewIborIndex(itofin.IborIndexConfig{FamilyName: "example-EUR", Tenor: itofin.Period{Length: 3, Unit: itofin.Months}, Currency: eur, FixingCalendar: calendar, Convention: itofin.Unadjusted, DayCounter: dc, Forwarding: bc, Settings: settings}))
	qi := must(s.NewIborIndex(itofin.IborIndexConfig{FamilyName: "example-USD", Tenor: itofin.Period{Length: 3, Unit: itofin.Months}, Currency: usd, FixingCalendar: calendar, Convention: itofin.Unadjusted, DayCounter: dc, Forwarding: qc, Settings: settings}))
	sch := must(s.NewSchedule(itofin.ScheduleConfig{Start: must(itofin.NewDate(8, 1, 2026)), End: must(itofin.NewDate(8, 1, 2027)), Frequency: itofin.Quarterly, Calendar: calendar, Convention: itofin.Unadjusted}))
	swap := must(s.NewXccyBasisSwap(itofin.XccyBasisSwapConfig{BaseSchedule: sch, QuoteSchedule: sch, BaseIndex: bi, QuoteIndex: qi, BaseCurve: bc, QuoteCurve: qc, BaseDayCounter: dc, QuoteDayCounter: dc, BaseCurrency: eur, QuoteCurrency: usd, BaseNotional: 1e6, QuoteNotional: 1.27e6, QuoteSpread: .0017, Spot: 1.27, Settings: settings}))
	fmt.Println("EUR NPV", must(swap.NPV()), "USD NPV", must(swap.NPVInQuote()), "fair spread", must(swap.FairQuoteBasisSpread()))
}
