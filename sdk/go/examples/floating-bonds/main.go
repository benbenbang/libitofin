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
	today, err := itofin.NewDate(15, 1, 2024)
	if err != nil {
		return err
	}
	start, err := itofin.NewDate(15, 7, 2024)
	if err != nil {
		return err
	}
	end, err := itofin.NewDate(15, 7, 2026)
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
	calendar, err := s.NullCalendar()
	if err != nil {
		return err
	}
	counter, err := s.Actual365Fixed()
	if err != nil {
		return err
	}
	couponCounter, err := s.Actual360()
	if err != nil {
		return err
	}
	currency, err := s.USD()
	if err != nil {
		return err
	}
	forward, err := s.NewFlatForward(today, .03, counter)
	if err != nil {
		return err
	}
	discount, err := s.NewFlatForward(today, .04, counter)
	if err != nil {
		return err
	}
	index, err := s.NewIborIndex(itofin.IborIndexConfig{FamilyName: "FloatingExample", Tenor: itofin.Period{Length: 6, Unit: itofin.Months}, SettlementDays: 2, Currency: currency, FixingCalendar: calendar, Convention: itofin.ModifiedFollowing, DayCounter: couponCounter, Forwarding: forward, Settings: settings})
	if err != nil {
		return err
	}
	schedule, err := s.NewSchedule(itofin.ScheduleConfig{Start: start, End: end, Frequency: itofin.Semiannual, Calendar: calendar, Convention: itofin.Unadjusted})
	if err != nil {
		return err
	}
	bond, err := s.NewFloatingRateBond(itofin.FloatingRateBondConfig{SettlementDays: 2, FaceAmount: 1000, Schedule: schedule, Index: index, AccrualDayCounter: couponCounter, Settings: settings, IssueDate: &start})
	if err != nil {
		return err
	}
	engine, err := s.NewDiscountingBondEngine(discount, settings, nil)
	if err != nil {
		return err
	}
	if err = bond.SetEngine(engine); err != nil {
		return err
	}
	price, err := bond.CleanPrice()
	if err != nil {
		return err
	}
	fmt.Printf("Floating clean price per 100: %.12f\n", price)
	return nil
}
