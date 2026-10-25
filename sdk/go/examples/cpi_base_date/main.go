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
func check(err error) {
	if err != nil {
		log.Fatal(err)
	}
}
func main() {
	s := must(itofin.NewSession())
	defer s.Close()
	date := func(day, month, year int) itofin.Date { return must(itofin.NewDate(day, month, year)) }
	today := date(15, 6, 2025)
	settings := must(s.NewSettings())
	check(settings.SetEvaluationDate(today))
	index := must(s.NewUKRPI(settings))
	check(index.AddFixing(date(1, 4, 2025), 100.8))
	check(index.AddFixing(date(1, 10, 2024), 99.5))
	dc := must(s.Actual365Fixed())
	calendar := must(s.NullCalendar())
	schedule := must(s.NewSchedule(itofin.ScheduleConfig{Start: date(15, 1, 2025), End: date(15, 7, 2025), Frequency: itofin.Semiannual, Calendar: calendar, Convention: itofin.Unadjusted}))
	builder := must(s.NewCPILegFromBaseDate(itofin.CPILegConfig{Schedule: schedule, Index: index, BaseCPI: 100, ObservationLag: itofin.Period{Length: 3, Unit: itofin.Months}, Interpolation: itofin.CpiFlat, Notionals: []float64{100000}, FixedRates: []float64{.02}, DayCounter: dc, PaymentCalendar: calendar, PaymentAdjustment: itofin.Unadjusted, GrowthOnly: false}, nil))
	leg := must(builder.Build())
	discount := must(s.NewFlatForward(today, .04, dc))
	for i := 0; i < must(leg.Len()); i++ {
		flow := must(leg.At(i))
		fmt.Printf("flow%d %.8f\n", i, must(flow.Amount()))
	}
	fmt.Printf("NPV %.8f\n", must(leg.NPV(itofin.CashFlowsNPVConfig{Discount: discount, Settings: settings})))
}
