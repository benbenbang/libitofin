package main

import (
	"errors"
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() (err error) {
	session, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, session.Close()) }()
	settings, err := session.NewSettings()
	if err != nil {
		return err
	}
	calendar, err := session.NullCalendar()
	if err != nil {
		return err
	}
	counter, err := session.Actual360()
	if err != nil {
		return err
	}
	curveCounter, err := session.Actual365Fixed()
	if err != nil {
		return err
	}
	start, err := itofin.NewDate(24, 3, 2026)
	if err != nil {
		return err
	}
	end, err := itofin.NewDate(24, 3, 2028)
	if err != nil {
		return err
	}
	reference, err := itofin.NewDate(1, 6, 2026)
	if err != nil {
		return err
	}
	settlement, err := itofin.NewDate(26, 6, 2026)
	if err != nil {
		return err
	}
	schedule, err := session.NewSchedule(itofin.ScheduleConfig{Start: start, End: end,
		Frequency: itofin.Semiannual, Calendar: calendar, Convention: itofin.Unadjusted})
	if err != nil {
		return err
	}
	bond, err := session.NewFixedRateBond(itofin.FixedRateBondConfig{
		BondConfig: itofin.BondConfig{SettlementDays: 5, FaceAmount: 375, Settings: settings},
		Schedule:   schedule, Coupons: []float64{.047, .058}, AccrualDayCounter: counter})
	if err != nil {
		return err
	}
	curve, err := session.NewFlatForward(reference, .032, curveCounter)
	if err != nil {
		return err
	}
	for _, query := range []struct {
		name  string
		value func() (float64, error)
	}{
		{"Clean100 target ATM", func() (float64, error) { return bond.ATMRateFromPrice(curve, 100, itofin.Clean, &settlement) }},
		{"Dirty100 target ATM", func() (float64, error) { return bond.ATMRateFromPrice(curve, 100, itofin.Dirty, &settlement) }},
		{"Own-NPV ATM coupon rate", func() (float64, error) { return bond.ATMRateFromCurve(curve, &settlement) }},
	} {
		value, err := query.value()
		if err != nil {
			return err
		}
		fmt.Printf("%s: %.12f\n", query.name, value)
	}
	return nil
}
