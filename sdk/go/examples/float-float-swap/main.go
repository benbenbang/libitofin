package main

import (
	"errors"
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"log"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
func run() (err error) {
	s, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, s.Close()) }()
	today, err := itofin.NewDate(2, 1, 2026)
	if err != nil {
		return err
	}
	start, err := itofin.NewDate(17, 1, 2026)
	if err != nil {
		return err
	}
	end, err := itofin.NewDate(17, 1, 2027)
	if err != nil {
		return err
	}
	settings, err := s.NewSettings()
	if err != nil {
		return err
	}
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	dc, err := s.Actual360()
	if err != nil {
		return err
	}
	dc365, err := s.Actual365Fixed()
	if err != nil {
		return err
	}
	currency, err := s.EUR()
	if err != nil {
		return err
	}
	calendar, err := s.NullCalendar()
	if err != nil {
		return err
	}
	weekends, err := s.WeekendsOnly()
	if err != nil {
		return err
	}
	curves := make([]*itofin.YieldTermStructure, 3)
	for i, rate := range []float64{.025, .034, .02} {
		curves[i], err = s.NewFlatForward(today, rate, dc)
		if err != nil {
			return err
		}
	}
	indexes := make([]*itofin.IborIndex, 2)
	schedules := make([]*itofin.Schedule, 2)
	for i, tenor := range []int32{3, 6} {
		indexes[i], err = s.NewIborIndex(itofin.IborIndexConfig{FamilyName: fmt.Sprintf("ExampleFloat%d", i), Tenor: itofin.Period{Length: tenor, Unit: itofin.Months}, Currency: currency, FixingCalendar: calendar, Convention: itofin.Unadjusted, DayCounter: dc, Forwarding: curves[i], Settings: settings})
		if err != nil {
			return err
		}
		frequency := itofin.Quarterly
		if i == 1 {
			frequency = itofin.Semiannual
		}
		schedules[i], err = s.NewSchedule(itofin.ScheduleConfig{Start: start, End: end, Frequency: frequency, Calendar: weekends, Convention: itofin.Unadjusted})
		if err != nil {
			return err
		}
	}
	swap, err := s.NewFloatFloatSwap(itofin.FloatFloatSwapConfig{Nominal: 1_000_000, PaySchedule: schedules[0], PayIndex: indexes[0], PaySpread: .001, PayDayCounter: dc, ReceiveSchedule: schedules[1], ReceiveIndex: indexes[1], ReceiveSpread: -.002, ReceiveDayCounter: dc365, Settings: settings})
	if err != nil {
		return err
	}
	if err = swap.SetEngine(curves[2], settings); err != nil {
		return err
	}
	value, err := swap.NPV()
	if err != nil {
		return err
	}
	fair, err := swap.FairPaySpread()
	if err != nil {
		return err
	}
	fmt.Printf("NPV: %.10f\nabsolute paid fair spread: %.10f\n", value, fair)
	return nil
}
